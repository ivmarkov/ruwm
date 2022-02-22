#![feature(type_alias_impl_trait)]
#![feature(generic_associated_types)]

use std::env;

extern crate alloc;
use alloc::sync::Arc;

use futures::try_join;

use log::info;

use embedded_graphics::prelude::{Dimensions, Point, Size};
use embedded_graphics::primitives::Rectangle;

use display_interface_spi::SPIInterfaceNoCS;

use embedded_hal::digital::v2::OutputPin;

use embedded_svc::event_bus::nonblocking::{EventBus as _, PostboxProvider};
use embedded_svc::timer::nonblocking::TimerService;
use embedded_svc::unblocker::nonblocking::Unblocker;
use embedded_svc::utils::nonblocking::channel::adapt;
use embedded_svc::utils::nonblocking::event_bus::{AsyncPostbox, AsyncSubscription};
use embedded_svc::utils::nonblocking::{Asyncify, UnblockingAsyncify};
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};

use esp_idf_hal::gpio::Pull;
use esp_idf_hal::mutex::Mutex;
use esp_idf_hal::prelude::*;
use esp_idf_hal::{adc, delay, gpio, spi};

use esp_idf_svc::eventloop::{
    EspBackgroundEventLoop, EspEventLoop, EspEventLoopType, EspSubscription,
    EspTypedEventDeserializer, EspTypedEventLoop, EspTypedEventSerializer,
};
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::netif::EspNetifStack;
use esp_idf_svc::nvs::EspDefaultNvs;
use esp_idf_svc::sysloop::EspSysLoopStack;
use esp_idf_svc::timer::EspTimerService;
use esp_idf_svc::wifi::EspWifi;

use esp_idf_sys::EspError;

use event::{
    ButtonCommandEvent, DrawRequestEvent, ValveSpinCommandEvent, ValveSpinNotifEvent,
    WifiStatusNotifEvent,
};

use pulse_counter::PulseCounter;

use ruwm::battery::{self, BatteryState};
use ruwm::broadcast_event::*;
use ruwm::button::{Button, PressedLevel};
use ruwm::pulse_counter::PulseCounter as _;
use ruwm::screen::{CroppedAdaptor, DrawEngine, DrawRequest, FlushableAdaptor, Screen};
use ruwm::state_snapshot::StateSnapshot;
use ruwm::storage::Storage;
use ruwm::valve::{self, ValveCommand, ValveState};
use ruwm::water_meter::{self, WaterMeterCommand, WaterMeterState};
use ruwm::{emergency, pipe};
use ruwm::{event_logger, mqtt};

use ruwm_std::unblocker::SmolUnblocker;

use crate::event::{
    BatteryStateEvent, MqttClientNotificationEvent, MqttPublishNotificationEvent,
    ValveCommandEvent, ValveStateEvent, WaterMeterCommandEvent, WaterMeterStateEvent,
};

mod espidf;
mod event;
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

    //let broadcast = espidf::broadcast::broadcast(100)?;
    let (bc_sender, bc_receiver) = ruwm_std::broadcast::broadcast::<BroadcastEvent>(100)?;

    let mut event_loop = EspBackgroundEventLoop::new(&Default::default())?;

    let mut timer_service = EspTimerService::new()?.into_async();

    let event_logger = event_logger::run(bc_receiver.clone());

    let netif_stack = Arc::new(EspNetifStack::new()?);
    let sysloop_stack = Arc::new(EspSysLoopStack::new()?);
    let nvs_stack = Arc::new(EspDefaultNvs::new()?);

    let mut wifi = EspWifi::new(netif_stack, sysloop_stack, nvs_stack)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        password: PASS.into(),
        ..Default::default()
    }))?;

    let wifi_notif = pipe::run(
        wifi.as_async().subscribe()?,
        adapt::sender(bc_sender.clone(), |_| {
            Some(BroadcastEvent::new("WIFI", Payload::WifiStatus))
        }),
    );

    let valve_state = state::<Option<ValveState>>();
    let battery_state = state::<BatteryState>();
    let water_meter_state = state::<WaterMeterState>();

    let valve_power_pin = peripherals.pins.gpio10.into_output()?;
    let valve_open_pin = peripherals.pins.gpio12.into_output()?;
    let valve_close_pin = peripherals.pins.gpio13.into_output()?;

    let (vsc_sender, vsc_receiver) = espidf::notify::notify::<ValveCommand>()?;
    let (vsn_sender, vsn_receiver) = espidf::notify::notify::<()>()?;

    let valve = valve::run(
        valve_state.clone(),
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("VALVE", Payload::ValveState(p)))
        }),
        timer_service.timer()?,
        vsc_sender,
        vsc_receiver,
        vsn_sender,
        vsn_receiver,
        valve_power_pin,
        valve_open_pin,
        valve_close_pin,
    );

    let powered_adc1 = adc::PoweredAdc::new(
        peripherals.adc1,
        adc::config::Config::new().calibration(true),
    )?;

    let battery_pin = peripherals.pins.gpio35.into_analog_atten_11db()?;
    let power_pin = peripherals.pins.gpio14.into_input()?;

    let battery = battery::run(
        battery_state.clone(),
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("VALVE", Payload::BatteryState(p)))
        }),
        timer_service.timer()?,
        powered_adc1,
        battery_pin,
        power_pin,
    );

    let mut pulse_counter = PulseCounter::new(peripherals.ulp);

    pulse_counter.initialize()?;

    let water_meter = water_meter::run(
        water_meter_state.clone(),
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("WM", Payload::WaterMeterState(p)))
        }),
        timer_service.timer()?,
        pulse_counter,
    );

    let mut button1 = Button::new(
        1,
        peripherals.pins.gpio25.into_input()?.into_pull_up()?,
        timer_service.timer()?,
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("BUTTON1", Payload::ButtonCommand(p)))
        }),
        PressedLevel::Low,
    );

    let mut button2 = Button::new(
        2,
        peripherals.pins.gpio26.into_input()?.into_pull_up()?,
        timer_service.timer()?,
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("BUTTON2", Payload::ButtonCommand(p)))
        }),
        PressedLevel::Low,
    );

    let mut button3 = Button::new(
        3,
        peripherals.pins.gpio27.into_input()?.into_pull_up()?,
        timer_service.timer()?,
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("BUTTON1", Payload::ButtonCommand(p)))
        }),
        PressedLevel::Low,
    );

    let (draw_sender, draw_receiver) = espidf::notify::notify::<DrawRequest>()?;

    let mut screen = Screen::new(
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::receiver(bc_receiver.clone(), Into::into),
        valve_state.get(),
        water_meter_state.get(),
        battery_state.get(),
        draw_sender,
    );

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

    info!("Original size: {:?}", display.bounding_box());

    let mut draw_engine = DrawEngine::<SmolUnblocker, _, _>::new(
        draw_receiver,
        // The TTGO board's screen does not start at offset 0x0, and the physical size is 135x240, instead of 240x320
        FlushableAdaptor::noop(CroppedAdaptor::new(
            Rectangle::new(Point::new(52, 40), Size::new(135, 240)),
            display,
        )),
    );

    let mqtt_conf = MqttClientConfiguration {
        client_id: Some("water-meter-demo"),
        ..Default::default()
    };

    let (mqtt_client, mqtt_connection) =
        EspMqttClient::new_async::<SmolUnblocker, _>("mqtt://broker.emqx.io:1883", &mqtt_conf)?;

    let topic_prefix = "water-meter-demo";

    let mut mqtt = mqtt::Mqtt::new(
        mqtt_client,
        mqtt_connection,
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new(
                "MQTT",
                Payload::MqttPublishNotification(p),
            ))
        }),
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new(
                "MQTT",
                Payload::MqttClientNotification(p),
            ))
        }),
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("MQTT", Payload::ValveCommand(p)))
        }),
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("MQTT", Payload::WaterMeterCommand(p)))
        }),
    );

    let emergency = emergency::run(
        adapt::sender(bc_sender.clone(), |p| {
            Some(BroadcastEvent::new("EMERGENCY", Payload::ValveCommand(p)))
        }),
        adapt::receiver(bc_receiver.clone(), Into::into),
        adapt::receiver(bc_receiver.clone(), Into::into),
    );

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
            mqtt.run(topic_prefix),
            button1.run(),
            button2.run(),
            button3.run(),
            screen.run(),
            draw_engine.run(),
        }
    })?;

    Ok(())
}
