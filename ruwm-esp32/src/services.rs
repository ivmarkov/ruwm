use core::fmt::Debug;
use core::mem;

extern crate alloc;

use channel_bridge::asynch::ws::WsError;

use edge_frame::assets::{self, serve::Asset};
use edge_http::io::{self, server::DefaultServer};
use edge_http::Method;
use edge_ws::io::WsConnection;

use embassy_time::Duration;

use embedded_nal_async::{Ipv4Addr, SocketAddr, SocketAddrV4};
use embedded_nal_async_xtra::{TcpListen, TcpSplittableConnection};

use embedded_hal::digital::OutputPin as EHOutputPin;

use embedded_io_async::{Read, Write};
use embedded_svc::http::server::asynch::Request;
use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::wifi::asynch::Wifi;
use embedded_svc::wifi::{AuthMethod, ClientConfiguration, Configuration};

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

use esp_idf_svc::mqtt::client::{EspAsyncMqttClient, MqttClientConfiguration};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::timer::EspTaskTimerService;
use esp_idf_svc::wifi::{AsyncWifi, BlockingWifi, EspWifi};

use esp_idf_svc::sys::{adc_atten_t, EspError};

use gfx_xtra::draw_target::{Flushable, OwnedDrawTargetExt};

use ruwm::button::PressedLevel;
use ruwm::pulse_counter::PulseCounter;
use ruwm::pulse_counter::PulseWakeup;
use ruwm::screen::Color;
use ruwm::valve::{self, ValveState};
use ruwm::wm::WaterMeterState;
use ruwm::wm_stats::WaterMeterStatsState;

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
            ssid: SSID.try_into().unwrap(),
            auth_method: AuthMethod::None,
            ..Default::default()
        }))?;
    } else {
        wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: SSID.try_into().unwrap(),
            password: PASS.try_into().unwrap(),
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

#[derive(Debug)]
enum HttpdError<T> {
    Http(io::Error<T>),
    Ws(WsError<T>),
}

impl<T> From<io::Error<T>> for HttpdError<T> {
    fn from(err: io::Error<T>) -> Self {
        Self::Http(err)
    }
}

impl<T> From<WsError<T>> for HttpdError<T> {
    fn from(err: WsError<T>) -> Self {
        Self::Ws(err)
    }
}

struct HttpdHandler<'a>(&'a [Asset]);

impl<'a> HttpdHandler<'a> {
    fn new(assets: &'a [Asset]) -> Self {
        Self(assets)
    }

    async fn handle<'b, T, const N: usize>(
        &self,
        handler_id: usize,
        con: &mut io::server::Connection<'b, T, N>,
    ) -> Result<(), HttpdError<T::Error>>
    where
        T: Read + Write + TcpSplittableConnection,
    {
        if matches!(con.headers().unwrap().method, Some(Method::Get)) {
            if matches!(con.headers()?.path, Some("/ws")) {
                self.handle_ws(handler_id, con).await?;
            } else {
                self.handle_assets(con).await?;
            }
        } else {
            con.initiate_response(405, None, &[]).await?;
        }

        Ok(())
    }

    async fn handle_ws<'b, T, const N: usize>(
        &self,
        handler_id: usize,
        con: &mut io::server::Connection<'b, T, N>,
    ) -> Result<(), HttpdError<T::Error>>
    where
        T: Read + Write + TcpSplittableConnection,
    {
        if con.is_ws_upgrade_request()? {
            con.initiate_ws_upgrade_response().await?;
            con.complete().await?;

            let socket = con.unbind()?;

            let (read, write) = socket.split().map_err(io::Error::Io)?;

            let sender = WsConnection::new(write, || None);
            let receiver = WsConnection::new(read, || Option::<()>::None);

            ruwm::ws::handle(sender, receiver, handler_id)
                .await
                .unwrap(); // TODO
        } else {
            con.initiate_response(200, None, &[]).await?;
        }

        Ok(())
    }

    async fn handle_assets<'b, T, const N: usize>(
        &self,
        con: &mut io::server::Connection<'b, T, N>,
    ) -> Result<(), io::Error<T::Error>>
    where
        T: Read + Write,
    {
        let asset = self
            .0
            .iter()
            .find(|asset| Some(Some(asset.0)) == con.headers().ok().map(|headers| headers.path));

        if let Some(asset) = asset {
            assets::serve::asynch::serve(Request::wrap(con), *asset).await
        } else {
            con.initiate_response(404, None, &[]).await
        }
    }
}

impl<'a, 'b, T, const N: usize> io::server::TaskHandler<'a, T, N> for HttpdHandler<'b>
where
    T: Read + Write + TcpSplittableConnection,
{
    type Error = HttpdError<T::Error>;

    async fn handle(
        &self,
        handler_id: usize,
        con: &mut io::server::Connection<'a, T, N>,
    ) -> Result<(), Self::Error> {
        HttpdHandler::handle(self, handler_id, con).await
    }
}

pub async fn run_httpd(server: &mut DefaultServer) -> Result<(), InitError> {
    let stack = edge_std_nal_async::Stack::new();

    let acceptor = stack
        .listen(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 80)))
        .await
        .unwrap(); // TODO

    server
        .run_with_task_id(acceptor, HttpdHandler::new(&ASSETS))
        .await
        .unwrap(); // TODO

    Ok(())
}

pub fn mqtt() -> Result<
    (
        &'static str,
        impl Client + Publish,
        impl Connection + 'static,
    ),
    InitError,
> {
    let client_id = "water-meter-demo";
    let (mqtt_client, mqtt_conn) = EspAsyncMqttClient::new(
        "mqtt://broker.emqx.io:1883",
        &MqttClientConfiguration {
            client_id: Some(client_id),
            ..Default::default()
        },
    )?;

    Ok((client_id, mqtt_client, mqtt_conn))
}
