#![feature(cfg_version)]
#![cfg_attr(not(version("1.65")), feature(generic_associated_types))]
#![feature(type_alias_impl_trait)]

use core::fmt::Debug;
use std::cell::RefCell;

extern crate alloc;

use edge_executor::SpawnError;

use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{queue::Queue, Duration};

use mipidsi::{Display, DisplayOptions};
use static_cell::StaticCell;

use embedded_graphics::prelude::RgbColor;

use display_interface_spi::SPIInterfaceNoCS;

use embedded_svc::http::server::Method;
use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::utils::asyncify::Asyncify;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, Wifi};
use embedded_svc::ws::asynch::server::Acceptor;

use esp_idf_hal::cs::embassy_sync::CriticalSectionRawMutex;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::executor::{CurrentTaskWait, TaskHandle};
use esp_idf_hal::gpio::*;
use esp_idf_hal::modem::WifiModemPeripheral;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi::*;
use esp_idf_hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_hal::{adc::*, delay};

use esp_idf_svc::errors::EspIOError;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::ws::{EspHttpWsAsyncConnection, EspHttpWsProcessor};
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::wifi::{EspWifi, WifiEvent, WifiWait};

use esp_idf_sys::{esp, EspError};

use edge_frame::assets;

use ruwm::button::PressedLevel;
use ruwm::channel::Receiver;
use ruwm::mqtt::{MessageParser, MqttCommand};
use ruwm::pulse_counter::PulseCounter;
use ruwm::pulse_counter::PulseWakeup;
use ruwm::screen::{FlushableAdaptor, FlushableDrawTarget};
use ruwm::state::{PostcardSerDe, PostcardStorage};
use ruwm::system::{SlowMem, System};
use ruwm::utils::EventBusReceiver;
use ruwm::valve;
use ruwm::web;

#[cfg(feature = "ulp")]
mod ulp_pulse_counter;

const SSID: &str = env!("RUWM_WIFI_SSID");
const PASS: &str = env!("RUWM_WIFI_PASS");

const ASSETS: assets::serve::Assets = edge_frame::assets!("RUWM_WEB");
const SLEEP_TIME: Duration = Duration::from_secs(30);
const WS_MAX_CONNECTIONS: usize = 2;
const MQTT_MAX_TOPIC_LEN: usize = 64;

type EspSystem =
    System<WS_MAX_CONNECTIONS, CriticalSectionRawMutex, EspStorage, EspHttpWsAsyncConnection<()>>;
type EspStorage = PostcardStorage<500, EspNvs<NvsDefault>>;

#[derive(Debug)]
pub enum InitError {
    EspError(EspError),
    SpawnError(SpawnError),
}

impl From<EspError> for InitError {
    fn from(e: EspError) -> Self {
        Self::EspError(e)
    }
}

impl From<EspIOError> for InitError {
    fn from(e: EspIOError) -> Self {
        Self::EspError(e.0)
    }
}

impl From<SpawnError> for InitError {
    fn from(e: SpawnError) -> Self {
        Self::SpawnError(e)
    }
}

//impl std::error::Error for InitError {}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum SleepWakeupReason {
    Unknown,
    ULP,
    Button,
    Timer,
    Other(u32),
}

// TODO: Linker issues
embassy_time::generic_queue!(static TIMER_QUEUE: Queue<128, esp_idf_hal::interrupt::embassy_sync::CriticalSectionRawMutex> = Queue::new());
//embassy_time::time_driver_impl!(static DRIVER: esp_idf_hal::timer::embassy_time::EspDriver = esp_idf_hal::timer::embassy_time::EspDriver::new());
embassy_time::time_driver_impl!(static DRIVER: esp_idf_svc::timer::embassy_time::EspDriver = esp_idf_svc::timer::embassy_time::EspDriver::new());
critical_section::set_impl!(esp_idf_hal::cs::critical_section::EspCriticalSection);

fn main() -> Result<(), InitError> {
    let wakeup_reason = get_sleep_wakeup_reason();

    init()?;

    log::info!("Wakeup reason: {:?}", wakeup_reason);

    run(wakeup_reason)?;

    sleep()?;

    unreachable!()
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

    unsafe {
        embassy_time::queue::initialize();
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

fn run(wakeup_reason: SleepWakeupReason) -> Result<(), InitError> {
    let peripherals = Peripherals::take().unwrap();

    // Valve pins

    let mut valve_power_pin = PinDriver::input_output(peripherals.pins.gpio25)?;
    let mut valve_open_pin = PinDriver::input_output(peripherals.pins.gpio26)?;
    let mut valve_close_pin = PinDriver::input_output(peripherals.pins.gpio27)?;

    valve_power_pin.set_pull(Pull::Floating)?;
    valve_open_pin.set_pull(Pull::Floating)?;
    valve_close_pin.set_pull(Pull::Floating)?;

    if wakeup_reason == SleepWakeupReason::ULP {
        valve::emergency_close(
            &mut valve_power_pin,
            &mut valve_open_pin,
            &mut valve_close_pin,
            &mut FreeRtos,
        );
    }

    // Button pins

    let button1_pin = peripherals.pins.gpio2;
    let button2_pin = peripherals.pins.gpio4;
    let button3_pin = peripherals.pins.gpio32;

    mark_wakeup_pins(&button1_pin, &button2_pin, &button3_pin)?;

    // ESP-IDF basics

    let nvs_default_partition = EspDefaultNvsPartition::take()?;
    let sysloop = EspSystemEventLoop::take()?;

    // System

    let system = system(nvs_default_partition.clone())?;

    // Pulse counter

    #[cfg(feature = "ulp")]
    let (pulse_counter, pulse_wakeup) = pulse(peripherals.ulp, peripherals.pins.gpio33)?;

    #[cfg(not(feature = "ulp"))]
    let (pulse_counter, pulse_wakeup) = pulse(peripherals.pins.gpio33)?;

    // Wifi

    let (wifi, wifi_notif) = wifi(
        peripherals.modem,
        sysloop.clone(),
        Some(nvs_default_partition),
    )?;

    // Httpd

    let (_httpd, ws_acceptor) = httpd()?;

    // Mqtt

    let (mqtt_topic_prefix, mqtt_client, mqtt_conn) = mqtt()?;

    // Display

    let display_backlight = peripherals.pins.gpio15;
    let display_dc = peripherals.pins.gpio18;
    let display_rst = peripherals.pins.gpio19;
    let display_spi = peripherals.spi2;
    let display_sclk = peripherals.pins.gpio14;
    let display_sdo = peripherals.pins.gpio13;
    let display_cs = peripherals.pins.gpio5;

    // High-prio executor

    let (mut executor1, tasks1) = system.spawn_executor0::<TaskHandle, CurrentTaskWait, _, _>(
        valve_power_pin,
        valve_open_pin,
        valve_close_pin,
        pulse_counter,
        pulse_wakeup,
        AdcDriver::new(peripherals.adc1, &AdcConfig::new().calibration(true))?,
        AdcChannelDriver::<_, Atten0dB<_>>::new(peripherals.pins.gpio36)?,
        PinDriver::input(peripherals.pins.gpio35)?,
        subscribe_pin(button1_pin, move || system.button1_signal())?,
        subscribe_pin(button2_pin, move || system.button1_signal())?,
        subscribe_pin(button3_pin, move || system.button1_signal())?,
    )?;

    // Mid-prio executor

    log::info!("Starting mid-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-mid\0"),
        stack_size: 10000,
        ..Default::default()
    }
    .set()
    .unwrap();

    let execution2 = system.schedule::<8, TaskHandle, CurrentTaskWait>(move || {
        system.spawn_executor1(
            display(
                display_backlight,
                display_dc,
                display_rst,
                display_spi,
                display_sclk,
                display_sdo,
                Some(display_cs),
            )
            .unwrap(),
            wifi,
            wifi_notif,
            mqtt_conn,
        )
    });

    // Low-prio executor

    log::info!("Starting low-prio executor");

    ThreadSpawnConfiguration {
        name: Some(b"async-exec-low\0"),
        stack_size: 20000,
        ..Default::default()
    }
    .set()
    .unwrap();

    let execution3 = system.schedule::<4, TaskHandle, CurrentTaskWait>(move || {
        system.spawn_executor2::<MQTT_MAX_TOPIC_LEN, _, _>(
            mqtt_topic_prefix,
            mqtt_client,
            ws_acceptor,
        )
    });

    // Start main execution

    log::info!("Starting high-prio executor");

    system.run(&mut executor1, tasks1);

    log::info!("Execution finished, waiting for 2s to workaround a STD/ESP-IDF pthread (?) bug");

    std::thread::sleep(core::time::Duration::from_millis(2000));

    execution2.join().unwrap();
    execution3.join().unwrap();

    log::info!("Finished execution");

    Ok(())
}

fn system(partition: EspDefaultNvsPartition) -> Result<&'static EspSystem, InitError> {
    static STORAGE: StaticCell<Mutex<CriticalSectionRawMutex, RefCell<EspStorage>>> =
        StaticCell::new();

    let storage = &*STORAGE.init(Mutex::new(RefCell::new(EspStorage::new(
        EspNvs::new(partition, "WM", true)?,
        PostcardSerDe,
    ))));

    static mut SLOW_MEM: Option<SlowMem> = None;
    unsafe { SLOW_MEM = Some(Default::default()) };

    static SYSTEM: StaticCell<EspSystem> = StaticCell::new();
    let system = &*SYSTEM.init(System::new(unsafe { SLOW_MEM.as_mut().unwrap() }, storage));

    Ok(system)
}

#[cfg(not(feature = "ulp"))]
fn pulse(
    pin: impl RTCPin + InputPin + OutputPin,
) -> Result<(impl PulseCounter, impl PulseWakeup), InitError> {
    static PULSE_SIGNAL: ruwm::notification::Notification = ruwm::notification::Notification::new();

    let pulse_counter = ruwm::pulse_counter::CpuPulseCounter::new(
        ruwm::utils::NotifReceiver::new(&PULSE_SIGNAL, &()),
        subscribe_pin(pin, || PULSE_SIGNAL.notify())?,
        PressedLevel::Low,
        Some(Duration::from_millis(50)),
    );

    Ok((pulse_counter, ()))
}

#[cfg(feature = "ulp")]
fn pulse(
    ulp: ULP,
    pin: impl RTCPin + InputPin + OutputPin,
) -> Result<(impl PulseCounter, impl PulseWakeup), InitError> {
    let mut pulse_counter = ulp_pulse_counter::UlpPulseCounter::new(
        esp_idf_hal::ulp::UlpDriver::new(ulp)?,
        pin,
        wakeup_reason == SleepWakeupReason::Unknown,
    )?;

    //let (pulse_counter, pulse_wakeup) = pulse_counter.split();

    Ok((pulse_counter, ()))
}

fn display<'d>(
    backlight: impl Peripheral<P = impl OutputPin> + 'd,
    dc: impl Peripheral<P = impl OutputPin> + 'd,
    rst: impl Peripheral<P = impl OutputPin> + 'd,
    spi: impl Peripheral<P = SPI2> + 'd,
    sclk: impl Peripheral<P = impl OutputPin> + 'd,
    sdo: impl Peripheral<P = impl OutputPin> + 'd,
    cs: Option<impl Peripheral<P = impl OutputPin> + 'd>,
) -> Result<impl FlushableDrawTarget<Color = impl RgbColor, Error = impl Debug> + 'd, InitError> {
    let mut backlight = PinDriver::output(backlight)?;

    backlight.set_drive_strength(DriveStrength::I40mA)?;
    backlight.set_high()?;

    let baudrate = 26.MHz().into();
    //let baudrate = 40.MHz().into();

    let di = SPIInterfaceNoCS::new(
        SpiMasterDriver::<SPI2>::new(
            spi,
            sclk,
            sdo,
            Option::<Gpio21>::None,
            cs,
            &SpiMasterConfig::new().baudrate(baudrate),
        )?,
        PinDriver::output(dc)?,
    );

    let rst = PinDriver::output(rst)?;

    #[cfg(feature = "ili9342")]
    let mut display = Display::ili9342c_rgb565(di, rst);

    #[cfg(feature = "st7789")]
    let mut display = Display::st7789_rgb565(di, rst);

    display
        .init(&mut delay::Ets, DisplayOptions::default())
        .unwrap();

    // The TTGO board's screen does not start at offset 0x0, and the physical size is 135x240, instead of 240x320
    #[cfg(feature = "ttgo")]
    let display = ruwm::screen::CroppedAdaptor::new(
        embedded_graphics::primitives::Rectangle::new(
            embedded_graphics::prelude::Point::new(52, 40),
            embedded_graphics::prelude::Size::new(135, 240),
        ),
        display,
    );

    let display = FlushableAdaptor::noop(display);

    Ok(display)
}

fn wifi<'d>(
    modem: impl Peripheral<P = impl WifiModemPeripheral + 'd> + 'd,
    mut sysloop: EspSystemEventLoop,
    partition: Option<EspDefaultNvsPartition>,
) -> Result<(impl Wifi + 'd, impl Receiver<Data = WifiEvent>), InitError> {
    let mut wifi = EspWifi::new(modem, sysloop.clone(), partition)?;

    if PASS.is_empty() {
        wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: SSID.into(),
            auth_method: AuthMethod::None,
            ..Default::default()
        }))?;
    } else {
        wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: SSID.into(),
            password: PASS.into(),
            ..Default::default()
        }))?;
    }

    let wifi_state_changed_source = sysloop.as_async().subscribe()?;

    let wait = WifiWait::new(&sysloop)?;

    wifi.start()?;

    wait.wait(|| wifi.is_started().unwrap());

    wifi.connect()?;

    //wait.wait(|| wifi.is_connected().unwrap());

    Ok((
        wifi,
        EventBusReceiver::<_, WifiEvent>::new(wifi_state_changed_source),
    ))
}

fn httpd() -> Result<
    (
        EspHttpServer,
        impl Acceptor<Connection = EspHttpWsAsyncConnection<()>>,
    ),
    InitError,
> {
    let (ws_processor, ws_acceptor) =
        EspHttpWsProcessor::<WS_MAX_CONNECTIONS, { web::WS_MAX_FRAME_LEN }>::new(());

    let ws_processor = Mutex::<CriticalSectionRawMutex, _>::new(RefCell::new(ws_processor));

    let mut httpd = EspHttpServer::new(&Default::default()).unwrap();

    for asset in &ASSETS {
        if !asset.0.is_empty() {
            httpd.fn_handler(asset.0, Method::Get, move |req| {
                assets::serve::serve(req, &asset)
            })?;
        }
    }

    httpd.ws_handler("/ws", move |connection| {
        ws_processor.lock(|ws_processor| ws_processor.borrow_mut().process(connection))
    })?;

    Ok((httpd, ws_acceptor))
}

fn mqtt() -> Result<
    (
        &'static str,
        impl Client + Publish,
        impl Connection<Message = Option<MqttCommand>>,
    ),
    InitError,
> {
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

    Ok((client_id, mqtt_client, mqtt_conn))
}

fn subscribe_pin<'d, P: InputPin + OutputPin>(
    pin: impl Peripheral<P = P> + 'd,
    notify: impl Fn() + Send + 'static,
) -> Result<impl embedded_hal::digital::v2::InputPin + 'd, InitError> {
    let mut pin = PinDriver::input(pin)?;

    pin.set_interrupt_type(InterruptType::NegEdge)?;

    unsafe {
        pin.subscribe(notify)?;
    }

    Ok(pin)
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
