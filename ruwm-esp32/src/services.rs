use core::cell::RefCell;
use core::fmt::Debug;
use core::future::Future;
use core::mem;

extern crate alloc;

use edge_frame::assets::serve::AssetMetadata;

use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Duration;

use embedded_hal::digital::OutputPin as EHOutputPin;

use embedded_svc::http::server::Method;
use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::utils::asyncify::Asyncify;
use embedded_svc::wifi::asynch::Wifi;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};
use embedded_svc::ws::asynch::server::Acceptor;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::adc::{Adc, AdcChannelDriver, AdcConfig, AdcDriver};
use esp_idf_svc::hal::delay;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::*;
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::hal::reset::WakeupReason;
use esp_idf_svc::hal::spi::*;
use esp_idf_svc::hal::task::embassy_sync::EspRawMutex;

use esp_idf_svc::http::server::ws::EspHttpWsProcessor;
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::timer::EspTaskTimerService;
use esp_idf_svc::wifi::{AsyncWifi, BlockingWifi, EspWifi};

use esp_idf_svc::sys::{adc_atten_t, EspError};

use gfx_xtra::draw_target::{Flushable, OwnedDrawTargetExt};

use edge_frame::assets;

use edge_executor::*;

use ruwm::button::PressedLevel;
use ruwm::mqtt::{MessageParser, MqttCommand};
use ruwm::pulse_counter::PulseCounter;
use ruwm::pulse_counter::PulseWakeup;
use ruwm::screen::Color;
use ruwm::valve::{self, ValveState};
use ruwm::wm::WaterMeterState;
use ruwm::wm_stats::WaterMeterStatsState;
use ruwm::ws;

use crate::errors::*;
use crate::peripherals::{DisplaySpiPeripherals, PulseCounterPeripherals, ValvePeripherals};

const SSID: &str = env!("RUWM_WIFI_SSID");
const PASS: &str = env!("RUWM_WIFI_PASS");

const ASSETS: assets::serve::Assets = edge_frame::assets!("RUWM_WEB");

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
) -> Result<(impl EHOutputPin, impl EHOutputPin, impl EHOutputPin), EspError> {
    let mut power = PinDriver::output(peripherals.power)?;
    let mut open = PinDriver::output(peripherals.open)?;
    let mut close = PinDriver::output(peripherals.close)?;

    // TODO: Do we need this?
    // power.set_pull(Pull::Floating)?;
    // open.set_pull(Pull::Floating)?;
    // close.set_pull(Pull::Floating)?;

    if wakeup_reason == WakeupReason::ULP {
        valve::emergency_close(&mut power, &mut open, &mut close, &mut FreeRtos);
    }

    Ok((power, open, close))
}

#[cfg(feature = "nvs")]
pub fn storage(
    partition: EspDefaultNvsPartition,
) -> Result<
    &'static Mutex<
        impl embassy_sync::blocking_mutex::raw::RawMutex,
        RefCell<impl embedded_svc::storage::Storage>,
    >,
    InitError,
> {
    const POSTCARD_BUF_SIZE: usize = 500;

    struct PostcardSerDe;

    impl embedded_svc::storage::SerDe for PostcardSerDe {
        type Error = postcard::Error;

        fn serialize<'a, T>(&self, slice: &'a mut [u8], value: &T) -> Result<&'a [u8], Self::Error>
        where
            T: serde::Serialize,
        {
            postcard::to_slice(value, slice).map(|r| &*r)
        }

        fn deserialize<T>(&self, slice: &[u8]) -> Result<T, Self::Error>
        where
            T: serde::de::DeserializeOwned,
        {
            postcard::from_bytes(slice)
        }
    }

    static STORAGE: static_cell::StaticCell<
        Mutex<
            EspRawMutex,
            RefCell<
                embedded_svc::storage::StorageImpl<
                    { POSTCARD_BUF_SIZE },
                    esp_idf_svc::nvs::EspDefaultNvs,
                    PostcardSerDe,
                >,
            >,
        >,
    > = static_cell::StaticCell::new();

    let storage = &*STORAGE.init(Mutex::new(RefCell::new(
        embedded_svc::storage::StorageImpl::new(
            esp_idf_svc::nvs::EspNvs::new(partition, "WM", true)?,
            PostcardSerDe,
        ),
    )));

    Ok(storage)
}

#[cfg(not(feature = "ulp"))]
pub fn pulse(
    peripherals: PulseCounterPeripherals<impl InputPin>,
) -> Result<(impl PulseCounter, impl PulseWakeup), InitError> {
    let pulse_counter = ruwm::pulse_counter::CpuPulseCounter::new(
        PinDriver::input(peripherals.pulse)?,
        PressedLevel::Low,
        Some(Duration::from_millis(50)),
    );

    Ok((pulse_counter, ()))
}

#[cfg(feature = "ulp")]
pub fn pulse(
    peripherals: PulseCounterPeripherals<impl RTCPin>,
    wakeup_reason: WakeupReason,
) -> Result<(impl PulseCounter, impl PulseWakeup), InitError> {
    let mut pulse_counter = ulp_pulse_counter::UlpPulseCounter::new(
        esp_idf_svc::hal::ulp::UlpDriver::new(ulp)?,
        peripherals.pulse,
        wakeup_reason == WakeupReason::Unknown,
    )?;

    //let (pulse_counter, pulse_wakeup) = pulse_counter.split();

    Ok((pulse_counter, ()))
}

pub fn button<'d, P: InputPin>(
    pin: impl Peripheral<P = P> + 'd,
) -> Result<impl embedded_hal::digital::InputPin + embedded_hal_async::digital::Wait + 'd, InitError>
{
    Ok(PinDriver::input(pin)?)
}

pub fn adc<'d, const A: adc_atten_t, ADC: Adc + 'd, P: ADCPin<Adc = ADC>>(
    adc: impl Peripheral<P = ADC> + 'd,
    pin: impl Peripheral<P = P> + 'd,
) -> Result<impl ruwm::battery::Adc + 'd, InitError> {
    struct AdcImpl<'d, const A: adc_atten_t, ADC, V>
    where
        ADC: Adc,
        V: ADCPin<Adc = ADC>,
    {
        driver: AdcDriver<'d, ADC>,
        channel_driver: AdcChannelDriver<'d, A, V>,
    }

    impl<'d, const A: adc_atten_t, ADC, V> ruwm::battery::Adc for AdcImpl<'d, A, ADC, V>
    where
        ADC: Adc,
        V: ADCPin<Adc = ADC>,
    {
        type Error = EspError;

        async fn read(&mut self) -> Result<u16, Self::Error> {
            self.driver.read(&mut self.channel_driver)
        }
    }

    Ok(AdcImpl {
        driver: AdcDriver::new(adc, &AdcConfig::new().calibration(true))?,
        channel_driver: AdcChannelDriver::<{ A }, _>::new(pin)?,
    })
}

pub fn display(
    peripherals: DisplaySpiPeripherals<impl Peripheral<P = impl SpiAnyPins + 'static> + 'static>,
) -> Result<impl Flushable<Color = Color, Error = impl Debug + 'static> + 'static, InitError> {
    if let Some(backlight) = peripherals.control.backlight {
        let mut backlight = PinDriver::output(backlight)?;

        backlight.set_drive_strength(DriveStrength::I40mA)?;
        backlight.set_high()?;

        mem::forget(backlight); // TODO: For now
    }

    let baudrate = 26.MHz().into();
    //let baudrate = 40.MHz().into();

    let spi = SpiDeviceDriver::new_single(
        peripherals.spi,
        peripherals.sclk,
        peripherals.sdo,
        Option::<Gpio21>::None,
        peripherals.cs,
        &SpiDriverConfig::new().dma(Dma::Disabled),
        &SpiConfig::new().baudrate(baudrate),
    )?;

    let dc = PinDriver::output(peripherals.control.dc)?;

    #[cfg(any(feature = "ili9342", feature = "st7789"))]
    let display = {
        let rst = PinDriver::output(peripherals.control.rst)?;

        #[cfg(feature = "ili9342")]
        let builder = mipidsi::Builder::ili9342c_rgb565(
            display_interface_spi::SPIInterfaceNoCS::new(spi, dc),
        );

        #[cfg(feature = "st7789")]
        let builder =
            mipidsi::Builder::st7789(display_interface_spi::SPIInterfaceNoCS::new(spi, dc));

        builder.init(&mut delay::Ets, Some(rst)).unwrap()
    };

    #[cfg(feature = "ssd1351")]
    let display = {
        use ssd1351::mode::displaymode::DisplayModeTrait;

        let mut display =
            ssd1351::mode::graphics::GraphicsMode::new(ssd1351::display::Display::new(
                ssd1351::interface::spi::SpiInterface::new(spi, dc),
                ssd1351::properties::DisplaySize::Display128x128,
                ssd1351::properties::DisplayRotation::Rotate0,
            ));

        display
            .reset(
                &mut PinDriver::output(peripherals.control.rst)?,
                &mut delay::Ets,
            )
            .unwrap();

        display
    };

    #[cfg(feature = "ttgo")]
    let mut display = {
        let rect = embedded_graphics::primitives::Rectangle::new(
            embedded_graphics::prelude::Point::new(52, 40),
            embedded_graphics::prelude::Size::new(135, 240),
        );

        display.owned_cropped(display, &rect)
    };

    let display = display.owned_noop_flushing().owned_color_converted();

    Ok(display)
}

// TODO: Make it async
pub fn wifi<'d>(
    modem: impl Peripheral<P = impl WifiModemPeripheral + 'd> + 'd,
    sysloop: EspSystemEventLoop,
    timer_service: EspTaskTimerService,
    partition: Option<EspDefaultNvsPartition>,
) -> Result<impl Wifi + 'd, InitError> {
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

    let mut bwifi = BlockingWifi::wrap(&mut wifi, sysloop.clone())?;

    bwifi.start()?;

    bwifi.connect()?;

    bwifi.wait_netif_up()?;

    let wifi = AsyncWifi::wrap(wifi, sysloop, timer_service)?;

    Ok(wifi)
}

pub fn httpd<'a>() -> Result<(EspHttpServer<'a>, impl Acceptor), InitError> {
    let (ws_processor, ws_acceptor) =
        EspHttpWsProcessor::<{ ws::WS_MAX_CONNECTIONS }, { ws::WS_MAX_FRAME_LEN }>::new(());

    let ws_processor = Mutex::<EspRawMutex, _>::new(RefCell::new(ws_processor));

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
        impl for<'a> Connection<Message<'a> = Option<MqttCommand>> + 'static,
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

pub fn schedule<'a, const C: usize>(
    stack_size: usize,
    run: impl Future + Send + 'static,
    spawner: impl FnOnce() -> LocalExecutor<'a, C> + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .stack_size(stack_size)
        .spawn(move || {
            let executor = spawner();

            // info!(
            //     "Tasks on thread {:?} scheduled, about to run the executor now",
            //     "TODO"
            // );

            block_on(executor.run(run));
        })
        .unwrap()
}
