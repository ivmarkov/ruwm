use core::cell::RefCell;
use core::fmt::Debug;
use core::mem;

extern crate alloc;

use serde::{de::DeserializeOwned, Serialize};

use edge_frame::assets::serve::AssetMetadata;

use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Duration;

use mipidsi::{Display, DisplayOptions};

use static_cell::StaticCell;

use embedded_graphics::prelude::RgbColor;

use display_interface_spi::SPIInterfaceNoCS;

use embedded_hal::digital::v2::OutputPin as EHOutputPin;

use embedded_svc::http::server::Method;
use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::storage::{SerDe, Storage, StorageImpl};
use embedded_svc::utils::asyncify::Asyncify;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, Wifi};
use embedded_svc::ws::asynch::server::Acceptor;

use esp_idf_hal::cs::embassy_sync::CriticalSectionRawMutex;
use esp_idf_hal::delay;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::*;
use esp_idf_hal::modem::WifiModemPeripheral;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_hal::reset::WakeupReason;
use esp_idf_hal::spi::*;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::ws::EspHttpWsProcessor;
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::wifi::{EspWifi, WifiEvent, WifiWait};

use esp_idf_sys::EspError;

use edge_frame::assets;

use ruwm::button::PressedLevel;
use ruwm::channel::{Channel, Receiver};
use ruwm::mqtt::{MessageParser, MqttCommand};
use ruwm::notification::Notification;
use ruwm::pulse_counter::PulseCounter;
use ruwm::pulse_counter::PulseWakeup;
use ruwm::screen::{FlushableAdaptor, FlushableDrawTarget};
use ruwm::valve::{self, ValveState};
use ruwm::web;
use ruwm::wm::WaterMeterState;
use ruwm::wm_stats::WaterMeterStatsState;

use crate::errors::*;
use crate::peripherals::{DisplaySpiPeripherals, PulseCounterPeripherals, ValvePeripherals};

const SSID: &str = env!("RUWM_WIFI_SSID");
const PASS: &str = env!("RUWM_WIFI_PASS");

const ASSETS: assets::serve::Assets = edge_frame::assets!("RUWM_WEB");

const POSTCARD_BUF_SIZE: usize = 500;

struct PostcardSerDe;

impl SerDe for PostcardSerDe {
    type Error = postcard::Error;

    fn serialize<'a, T>(&self, slice: &'a mut [u8], value: &T) -> Result<&'a [u8], Self::Error>
    where
        T: Serialize,
    {
        postcard::to_slice(value, slice).map(|r| &*r)
    }

    fn deserialize<T>(&self, slice: &[u8]) -> Result<T, Self::Error>
    where
        T: DeserializeOwned,
    {
        postcard::from_bytes(slice)
    }
}

#[derive(Default)]
pub struct RtcMemory {
    pub valve: Option<ValveState>,
    pub wm: WaterMeterState,
    pub wm_stats: WaterMeterStatsState,
}

impl RtcMemory {
    pub const fn new() -> Self {
        Self {
            valve: None,
            wm: WaterMeterState::new(),
            wm_stats: WaterMeterStatsState::new(),
        }
    }
}

#[cfg_attr(feature = "rtc-mem", link_section = ".rtc.data.rtc_memory")]
pub static mut RTC_MEMORY: RtcMemory = RtcMemory::new();

pub fn valve_pins(
    peripherals: ValvePeripherals,
    wakeup_reason: WakeupReason,
) -> Result<
    (
        impl EHOutputPin<Error = impl Debug>,
        impl EHOutputPin<Error = impl Debug>,
        impl EHOutputPin<Error = impl Debug>,
    ),
    EspError,
> {
    let mut power = PinDriver::input_output(peripherals.power)?;
    let mut open = PinDriver::input_output(peripherals.open)?;
    let mut close = PinDriver::input_output(peripherals.close)?;

    power.set_pull(Pull::Floating)?;
    open.set_pull(Pull::Floating)?;
    close.set_pull(Pull::Floating)?;

    if wakeup_reason == WakeupReason::ULP {
        valve::emergency_close(&mut power, &mut open, &mut close, &mut FreeRtos);
    }

    Ok((power, open, close))
}

pub fn storage(
    partition: EspDefaultNvsPartition,
) -> Result<&'static Mutex<CriticalSectionRawMutex, RefCell<impl Storage>>, InitError> {
    static STORAGE: StaticCell<
        Mutex<
            CriticalSectionRawMutex,
            RefCell<StorageImpl<{ POSTCARD_BUF_SIZE }, EspDefaultNvs, PostcardSerDe>>,
        >,
    > = StaticCell::new();

    let storage = &*STORAGE.init(Mutex::new(RefCell::new(StorageImpl::new(
        EspNvs::new(partition, "WM", true)?,
        PostcardSerDe,
    ))));

    Ok(storage)
}

#[cfg(not(feature = "ulp"))]
pub fn pulse(
    peripherals: PulseCounterPeripherals<impl RTCPin + InputPin + OutputPin>,
) -> Result<(impl PulseCounter, impl PulseWakeup), InitError> {
    static PULSE_SIGNAL: ruwm::notification::Notification = ruwm::notification::Notification::new();

    let pulse_counter = ruwm::pulse_counter::CpuPulseCounter::new(
        &PULSE_SIGNAL,
        subscribe_pin(peripherals.pulse, || PULSE_SIGNAL.notify())?,
        PressedLevel::Low,
        Some(Duration::from_millis(50)),
    );

    Ok((pulse_counter, ()))
}

#[cfg(feature = "ulp")]
pub fn pulse(
    peripherals: PulseCounterPeripherals<impl RTCPin + InputPin + OutputPin>,
    wakeup_reason: WakeupReason,
) -> Result<(impl PulseCounter, impl PulseWakeup), InitError> {
    let mut pulse_counter = ulp_pulse_counter::UlpPulseCounter::new(
        esp_idf_hal::ulp::UlpDriver::new(ulp)?,
        peripherals.pulse,
        wakeup_reason == WakeupReason::Unknown,
    )?;

    //let (pulse_counter, pulse_wakeup) = pulse_counter.split();

    Ok((pulse_counter, ()))
}

pub fn button<'d, P: InputPin + OutputPin>(
    pin: impl Peripheral<P = P> + 'd,
    notification: &'static Notification,
) -> Result<impl embedded_hal::digital::v2::InputPin + 'd, InitError> {
    subscribe_pin(pin, move || notification.notify())
}

pub fn display(
    peripherals: DisplaySpiPeripherals<impl Peripheral<P = impl SpiAnyPins + 'static> + 'static>,
) -> Result<impl FlushableDrawTarget<Color = impl RgbColor, Error = impl Debug> + 'static, InitError>
{
    if let Some(backlight) = peripherals.control.backlight {
        let mut backlight = PinDriver::output(backlight)?;

        backlight.set_drive_strength(DriveStrength::I40mA)?;
        backlight.set_high()?;

        mem::forget(backlight); // TODO: For now
    }

    let baudrate = 26.MHz().into();
    //let baudrate = 40.MHz().into();

    let di = SPIInterfaceNoCS::new(
        SpiMasterDriver::new(
            peripherals.spi,
            peripherals.sclk,
            peripherals.sdo,
            Option::<Gpio21>::None,
            peripherals.cs,
            &SpiMasterConfig::new().baudrate(baudrate),
        )?,
        PinDriver::output(peripherals.control.dc)?,
    );

    let rst = PinDriver::output(peripherals.control.rst)?;

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

pub fn wifi<'d>(
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

    Ok((wifi, Channel::new(wifi_state_changed_source)))
}

pub fn httpd() -> Result<(EspHttpServer, impl Acceptor), InitError> {
    let (ws_processor, ws_acceptor) =
        EspHttpWsProcessor::<{ web::WS_MAX_CONNECTIONS }, { web::WS_MAX_FRAME_LEN }>::new(());

    let ws_processor = Mutex::<CriticalSectionRawMutex, _>::new(RefCell::new(ws_processor));

    let mut httpd = EspHttpServer::new(&Default::default()).unwrap();

    let mut assets = ASSETS
        .iter()
        .filter(|asset| !asset.0.is_empty())
        .collect::<heapless::Vec<_, { assets::MAX_ASSETS }>>();

    assets.sort_by_key(|asset| AssetMetadata::derive(asset.0).uri);

    for asset in assets.iter().rev() {
        let asset = **asset;

        let metadata = AssetMetadata::derive(asset.0);

        httpd.fn_handler(metadata.uri, Method::Get, move |req| {
            assets::serve::serve(req, asset)
        })?;
    }

    httpd.ws_handler("/ws", move |connection| {
        ws_processor.lock(|ws_processor| ws_processor.borrow_mut().process(connection))
    })?;

    Ok((httpd, ws_acceptor))
}

pub fn mqtt() -> Result<
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
