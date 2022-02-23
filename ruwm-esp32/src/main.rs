#![feature(type_alias_impl_trait)]
#![feature(generic_associated_types)]

use std::env;

extern crate alloc;
use alloc::sync::Arc;

use futures::try_join;

use embedded_graphics::prelude::{Point, Size};
use embedded_graphics::primitives::Rectangle;

use display_interface_spi::SPIInterfaceNoCS;

use embedded_hal::digital::v2::OutputPin;

use embedded_svc::event_bus::nonblocking::EventBus;
use embedded_svc::utils::nonblocking::Asyncify;
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};

use esp_idf_hal::gpio::Pull;
use esp_idf_hal::mutex::Mutex;
use esp_idf_hal::prelude::*;
use esp_idf_hal::{adc, delay, gpio, spi};

use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::netif::EspNetifStack;
use esp_idf_svc::nvs::EspDefaultNvs;
use esp_idf_svc::sysloop::EspSysLoopStack;
use esp_idf_svc::wifi::EspWifi;

use pulse_counter::PulseCounter;

use ruwm::battery::BatteryState;
use ruwm::broadcast_binder;
use ruwm::pulse_counter::PulseCounter as _;
use ruwm::screen::{CroppedAdaptor, FlushableAdaptor};
use ruwm::state_snapshot::StateSnapshot;
use ruwm::storage::Storage;
use ruwm::valve::ValveState;
use ruwm::water_meter::WaterMeterState;

use ruwm_std::unblocker::SmolUnblocker;

use crate::espidf::notify::Notify;

mod espidf;
#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

const SSID: &str = env!("RUWM_WIFI_SSID");
const PASS: &str = env!("RUWM_WIFI_PASS");

fn state<S>() -> StateSnapshot<Mutex<S>>
where
    S: Send + Sync + Default,
{
    StateSnapshot::<Mutex<S>>::new()
}

fn main() -> anyhow::Result<()> {
    env::set_var("BLOCKING_MAX_THREADS", "2");

    esp_idf_sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let valve_state = state::<Option<ValveState>>();
    let battery_state = state::<BatteryState>();
    let water_meter_state = state::<WaterMeterState>();

    let (bc_sender, bc_receiver) =
        espidf::broadcast::broadcast::<espidf::broadcast_event_serde::Serde, _>(100)?;
    //let (bc_sender, bc_receiver) = ruwm_std::broadcast::broadcast::<BroadcastEvent>(100)?;

    let binder = broadcast_binder::BroadcastBinder::new(
        bc_sender.clone(),
        bc_receiver.clone(),
        espidf::timer::timers()?,
        Notify,
    );

    let netif_stack = Arc::new(EspNetifStack::new()?);
    let sysloop_stack = Arc::new(EspSysLoopStack::new()?);
    let nvs_stack = Arc::new(EspDefaultNvs::new()?);

    let mut wifi = EspWifi::new(netif_stack, sysloop_stack, nvs_stack)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        password: PASS.into(),
        ..Default::default()
    }))?;

    let event_logger = binder.event_logger()?;

    let wifi_notif = binder.wifi(wifi.as_async().subscribe()?)?;

    let valve = binder.valve(
        peripherals.pins.gpio10.into_output()?,
        peripherals.pins.gpio12.into_output()?,
        peripherals.pins.gpio13.into_output()?,
        valve_state,
    )?;

    let battery = binder.battery(
        battery_state.clone(),
        adc::PoweredAdc::new(
            peripherals.adc1,
            adc::config::Config::new().calibration(true),
        )?,
        peripherals.pins.gpio35.into_analog_atten_11db()?,
        peripherals.pins.gpio14.into_input()?,
    )?;

    let mut pulse_counter = PulseCounter::new(peripherals.ulp);

    pulse_counter.initialize()?;

    let water_meter = binder.water_meter(water_meter_state.clone(), pulse_counter)?;

    let button1 = binder.button(
        1,
        "BUTON1",
        peripherals.pins.gpio25.into_input()?.into_pull_up()?,
    )?;
    let button2 = binder.button(
        2,
        "BUTON2",
        peripherals.pins.gpio26.into_input()?.into_pull_up()?,
    )?;
    let button3 = binder.button(
        3,
        "BUTON3",
        peripherals.pins.gpio27.into_input()?.into_pull_up()?,
    )?;

    let backlight = peripherals.pins.gpio4;
    let dc = peripherals.pins.gpio16;
    let rst = peripherals.pins.gpio23;
    let spi = peripherals.spi2;
    let sclk = peripherals.pins.gpio18;
    let sdo = peripherals.pins.gpio19;
    let cs = peripherals.pins.gpio5;

    let mut backlight = backlight.into_output()?;
    backlight.set_high()?;

    let di = SPIInterfaceNoCS::new(
        spi::Master::<spi::SPI2, _, _, _, _>::new(
            spi,
            spi::Pins {
                sclk,
                sdo,
                sdi: Option::<gpio::Gpio21<gpio::Unknown>>::None,
                cs: Some(cs),
            },
            <spi::config::Config as Default>::default().baudrate(26.MHz().into()),
        )?,
        dc.into_output()?,
    );

    let mut display = st7789::ST7789::new(
        di,
        rst.into_output()?,
        // SP7789V is designed to drive 240x320 screens, even though the TTGO physical screen is smaller
        240,
        320,
    );

    display.init(&mut delay::Ets).unwrap();
    display
        .set_orientation(st7789::Orientation::Portrait)
        .unwrap();

    // The TTGO board's screen does not start at offset 0x0, and the physical size is 135x240, instead of 240x320
    let display = FlushableAdaptor::noop(CroppedAdaptor::new(
        Rectangle::new(Point::new(52, 40), Size::new(135, 240)),
        display,
    ));

    let screen = binder.screen(
        valve_state.get(),
        water_meter_state.get(),
        battery_state.get(),
        display,
    )?;

    let mqtt_conf = MqttClientConfiguration {
        client_id: Some("water-meter-demo"), // TODO
        ..Default::default()
    };

    let (mqtt_client, mqtt_connection) =
        EspMqttClient::new_async::<SmolUnblocker, _>("mqtt://broker.emqx.io:1883", &mqtt_conf)?;

    let mqtt = binder.mqtt(mqtt_conf.client_id.unwrap(), mqtt_client, mqtt_connection)?;

    let emergency = binder.emergency()?;

    esp_idf_sys::esp!(unsafe {
        esp_idf_sys::esp_vfs_eventfd_register(&esp_idf_sys::esp_vfs_eventfd_config_t {
            max_fds: 5,
            ..Default::default()
        })
    })?;

    smol::block_on(async move {
        try_join! {
            event_logger,
            valve,
            battery,
            water_meter,
            emergency,
            wifi_notif,
            mqtt,
            button1,
            button2,
            button3,
            screen,
        }
    })?;

    Ok(())
}
