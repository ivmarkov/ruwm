#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]
#![feature(explicit_generic_args_with_impl_trait)]
#![feature(nll)]

use core::fmt::Debug;
use core::time::Duration;

extern crate alloc;
use alloc::sync::Arc;

use embedded_graphics::prelude::{Point, RgbColor, Size};
use embedded_graphics::primitives::Rectangle;

use display_interface_spi::SPIInterfaceNoCS;

use embedded_hal::digital::v2::OutputPin;

use embedded_svc::event_bus::asynch::EventBus;
use embedded_svc::executor::asynch::{Executor, WaitableExecutor};
use embedded_svc::timer::asynch::TimerService;
use embedded_svc::utils::asynch::executor::SpawnError;
use embedded_svc::utils::asyncify::Asyncify;
use embedded_svc::utils::forever::Forever;
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi as WifiTrait};
use embedded_svc::ws::server::registry::Registry as _;

use esp_idf_hal::gpio::{self, InterruptType, Output, Pull, RTCPin};
use esp_idf_hal::mutex::RawMutex;
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi::SPI2;
use esp_idf_hal::{adc, delay, spi};

use esp_idf_svc::errors::EspIOError;
use esp_idf_svc::executor::asynch::isr::{local_tasks_spawner, tasks_spawner};
use esp_idf_svc::http::server::ws::asynch::{EspHttpWsAcceptor, EspHttpWsProcessor};
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::netif::EspNetifStack;
use esp_idf_svc::nvs::EspDefaultNvs;
use esp_idf_svc::sysloop::EspSysLoopStack;
use esp_idf_svc::systime::EspSystemTime;
use esp_idf_svc::timer::EspISRTimerService;
use esp_idf_svc::wifi::EspWifi;

use esp_idf_sys::{esp, EspError};

use edge_frame::assets::serve::*;

use pulse_counter::PulseCounter;

use ruwm::button::PressedLevel;
use ruwm::mqtt::MessageParser;
use ruwm::pulse_counter::PulseCounter as _;
use ruwm::screen::{CroppedAdaptor, FlushableAdaptor, FlushableDrawTarget};
use ruwm::system::{SlowMem, System};
use ruwm::valve::{self, ValveCommand};
use ruwm::water_meter::WaterMeterState;

#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

const SSID: &str = env!("RUWM_WIFI_SSID");
const PASS: &str = env!("RUWM_WIFI_PASS");

const ASSETS: Assets = edge_frame::assets!("RUWM_WEB");

const SLEEP_TIME: Duration = Duration::from_secs(30);

const MQTT_MAX_TOPIC_LEN: usize = 64;
const WS_MAX_CONNECTIONS: usize = 2;
const WS_MAX_FRAME_SIZE: usize = 4096;

fn main() -> Result<(), InitError> {
    let wakeup_reason = get_sleep_wakeup_reason();

    init()?;

    log::info!("Wakeup reason: {:?}", wakeup_reason);

    run(wakeup_reason)?;

    sleep()?;

    unreachable!()
}

fn run(wakeup_reason: SleepWakeupReason) -> Result<(), InitError> {
    let peripherals = Peripherals::take().unwrap();

    let mut valve_power_pin = peripherals.pins.gpio10.into_output()?;
    let mut valve_open_pin = peripherals.pins.gpio12.into_output()?;
    let mut valve_close_pin = peripherals.pins.gpio13.into_output()?;

    if wakeup_reason == SleepWakeupReason::ULP {
        emergency_valve_close(
            &mut valve_power_pin,
            &mut valve_open_pin,
            &mut valve_close_pin,
        );
    }

    let button1_pin = peripherals.pins.gpio35;
    let button2_pin = peripherals.pins.gpio0;
    let button3_pin = peripherals.pins.gpio27;

    mark_wakeup_pins(&button1_pin, &button2_pin, &button3_pin)?;

    static mut slow_mem: Option<SlowMem> = None;

    unsafe { slow_mem = Some(Default::default()) };

    static SYSTEM: Forever<System<RawMutex, EspHttpWsAcceptor<()>, WS_MAX_CONNECTIONS>> =
        Forever::new();
    let system = &*SYSTEM.put(System::new(unsafe { slow_mem.as_mut().unwrap() }));

    let mut timers = unsafe { EspISRTimerService::new() }?.into_async();

    let (mut executor1, tasks1) = local_tasks_spawner::<16, _>()
        .spawn_local(system.valve())?
        .spawn_local(system.valve_spin(
            timers.timer()?,
            valve_power_pin,
            valve_open_pin,
            valve_close_pin,
        ))?
        .spawn_local(system.wm(
            timers.timer()?,
            PulseCounter::new(peripherals.ulp).initialize()?,
        ))?
        .spawn_local(system.battery(
            timers.timer()?,
            adc::PoweredAdc::new(
                peripherals.adc1,
                adc::config::Config::new().calibration(true),
            )?,
            peripherals.pins.gpio33.into_analog_atten_11db()?,
            peripherals.pins.gpio14.into_input()?,
        ))?
        .spawn_local(system.button1(
            timers.timer()?,
            unsafe {
                button1_pin
                    .into_subscribed(move || system.button1_signal(), InterruptType::NegEdge)?
            },
            PressedLevel::Low,
        ))?
        .spawn_local(system.button2(
            timers.timer()?,
            unsafe {
                button2_pin
                    .into_subscribed(move || system.button2_signal(), InterruptType::NegEdge)?
                    .into_pull_up()?
            },
            PressedLevel::Low,
        ))?
        .spawn_local(system.button3(
            timers.timer()?,
            unsafe {
                button3_pin
                    .into_subscribed(move || system.button3_signal(), InterruptType::NegEdge)?
                    .into_pull_up()?
            },
            PressedLevel::Low,
        ))?
        .spawn_local(system.emergency())?
        .spawn_local(system.keepalive(timers.timer()?, EspSystemTime))?
        .release();

    let netif_stack = Arc::new(EspNetifStack::new()?);
    let sysloop_stack = Arc::new(EspSysLoopStack::new()?);
    let nvs_stack = Arc::new(EspDefaultNvs::new()?);
    let mut wifi = EspWifi::new(netif_stack, sysloop_stack, nvs_stack)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        password: PASS.into(),
        ..Default::default()
    }))?;

    let wifi_state_changed_source = wifi.as_async().subscribe()?;

    let client_id = "water-meter-demo";
    let mut mqtt_parser = MessageParser::new();

    let (mqtt_client, mqtt_conn) = EspMqttClient::new_with_converting_async_conn(
        "mqtt://broker.emqx.io:1883",
        &MqttClientConfiguration {
            client_id: Some(client_id),
            ..Default::default()
        },
        move |event| mqtt_parser.convert(event),
    )?;

    let mqtt_client = mqtt_client.into_async();

    let (ws_processor, ws_acceptor) =
        EspHttpWsProcessor::<WS_MAX_CONNECTIONS, WS_MAX_FRAME_SIZE>::new(());

    let ws_processor = esp_idf_hal::mutex::Mutex::new(ws_processor);

    let mut httpd = EspHttpServer::new(&Default::default()).unwrap();

    register_assets::<_, 128>(&mut httpd, &ASSETS)?;

    httpd.handle_ws("/ws", move |receiver, sender| {
        ws_processor.lock().process(receiver, sender)
    })?;

    let (mut executor2, tasks2) = tasks_spawner::<8, _>()
        .spawn(system.wm_stats(timers.timer().unwrap(), EspSystemTime))
        .unwrap()
        .spawn(system.screen())?
        .spawn(
            system.screen_draw(
                display(
                    peripherals.pins.gpio4.into_output()?.degrade(),
                    peripherals.pins.gpio16.into_output()?.degrade(),
                    peripherals.pins.gpio23.into_output()?.degrade(),
                    peripherals.spi2,
                    peripherals.pins.gpio18.into_output()?.degrade(),
                    peripherals.pins.gpio19.into_output()?.degrade(),
                    Some(peripherals.pins.gpio5.into_output()?.degrade()),
                )
                .unwrap(),
            ),
        )?
        .spawn(system.wifi(wifi, wifi_state_changed_source))?
        .spawn(system.mqtt_receive(mqtt_conn))?
        .spawn(system.web_receive::<WS_MAX_FRAME_SIZE>(ws_acceptor))?
        .release();

    // let (mut executor3, tasks3) = tasks_spawner::<4, _>()
    //     //.spawn(system.mqtt_send::<MQTT_MAX_TOPIC_LEN>(client_id, mqtt_client))?
    //     //.spawn(system.web_send::<WS_MAX_FRAME_SIZE>())?
    //     .release();

    log::info!("Starting execution");

    let executor2 = std::thread::spawn(move || {
        executor2.with_context(|exec, ctx| {
            exec.run(ctx, || system.should_quit(), Some(tasks2));
        });
    });

    // let executor3 = std::thread::spawn(move || {
    //     executor3.with_context(|exec, ctx| {
    //         exec.run(ctx, || system.should_quit(), Some(tasks3));
    //     });
    // });

    executor1.with_context(|exec, ctx| {
        exec.run(ctx, || system.should_quit(), Some(tasks1));
    });

    log::info!("Execution finished, waiting for 2s to workaround a STD/ESP-IDF pthread (?) bug");

    std::thread::sleep(Duration::from_millis(2000));

    executor2.join().unwrap();
    //executor3.join().unwrap();

    log::info!("Finished execution");

    Ok(())
}

fn init() -> Result<(), InitError> {
    esp_idf_sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    esp!(unsafe {
        #[allow(clippy::needless_update)]
        esp_idf_sys::esp_vfs_eventfd_register(&esp_idf_sys::esp_vfs_eventfd_config_t {
            max_fds: 5,
            ..Default::default()
        })
    })?;

    Ok(())
}

fn emergency_valve_close(
    power_pin: &mut impl OutputPin<Error = impl Debug>,
    open_pin: &mut impl OutputPin<Error = impl Debug>,
    close_pin: &mut impl OutputPin<Error = impl Debug>,
) {
    log::error!("Start: emergency closing valve due to ULP wakeup...");

    valve::start_spin(Some(ValveCommand::Close), power_pin, open_pin, close_pin);
    std::thread::sleep(valve::VALVE_TURN_DELAY);
    valve::start_spin(None, power_pin, open_pin, close_pin);

    log::error!("End: emergency closing valve due to ULP wakeup");
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum SleepWakeupReason {
    Unknown,
    ULP,
    Button,
    Timer,
    Other(u32),
}

fn get_sleep_wakeup_reason() -> SleepWakeupReason {
    match unsafe { esp_idf_sys::esp_sleep_get_wakeup_cause() } {
        esp_idf_sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_UNDEFINED => SleepWakeupReason::Unknown,
        esp_idf_sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_EXT1 => SleepWakeupReason::Button,
        esp_idf_sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_COCPU => SleepWakeupReason::ULP,
        esp_idf_sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_TIMER => SleepWakeupReason::Timer,
        other => SleepWakeupReason::Other(other),
    }
}

fn mark_wakeup_pins(
    button1_pin: &impl RTCPin,
    _button2_pin: &impl RTCPin,
    _button3_pin: &impl RTCPin,
) -> Result<(), InitError> {
    unsafe {
        esp!(esp_idf_sys::esp_sleep_enable_ext1_wakeup(
            1 << button1_pin.pin(),
            //| (1 << button2_pin.pin())
            //| (1 << button3_pin.pin())
            esp_idf_sys::esp_sleep_ext1_wakeup_mode_t_ESP_EXT1_WAKEUP_ALL_LOW,
        ))?;
    }

    Ok(())
}

fn sleep() -> Result<(), InitError> {
    unsafe {
        esp!(esp_idf_sys::esp_sleep_enable_ulp_wakeup())?;
        esp!(esp_idf_sys::esp_sleep_enable_timer_wakeup(
            SLEEP_TIME.as_micros() as u64
        ))?;

        log::info!("Going to sleep");

        esp_idf_sys::esp_deep_sleep_start();
    }

    Ok(())
}

fn display(
    mut backlight: gpio::GpioPin<Output>,
    dc: gpio::GpioPin<Output>,
    rst: gpio::GpioPin<Output>,
    spi: SPI2,
    sclk: gpio::GpioPin<Output>,
    sdo: gpio::GpioPin<Output>,
    cs: Option<gpio::GpioPin<Output>>,
) -> Result<impl FlushableDrawTarget<Color = impl RgbColor, Error = impl Debug>, InitError> {
    backlight.set_high()?;

    let di = SPIInterfaceNoCS::new(
        spi::Master::<SPI2, _, _, _, _>::new(
            spi,
            spi::Pins {
                sclk,
                sdo,
                sdi: Option::<gpio::Gpio21<gpio::Unknown>>::None,
                cs,
            },
            <spi::config::Config as Default>::default().baudrate(26.MHz().into()),
        )?,
        dc,
    );

    let mut display = st7789::ST7789::new(
        di, rst,
        // SP7789V is designed to drive 240x320 screens, even though the TTGO physical screen is smaller
        240, 320,
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

    Ok(display)
}

#[derive(Debug)]
pub enum InitError {
    EspError(EspError),
    EspIOError(EspIOError),
    SpawnError(SpawnError),
}

impl From<EspError> for InitError {
    fn from(e: EspError) -> Self {
        Self::EspError(e)
    }
}

impl From<EspIOError> for InitError {
    fn from(e: EspIOError) -> Self {
        Self::EspIOError(e)
    }
}

impl From<SpawnError> for InitError {
    fn from(e: SpawnError) -> Self {
        Self::SpawnError(e)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InitError {}
