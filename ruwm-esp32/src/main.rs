#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]

use core::fmt::Debug;
use core::time::Duration;
use std::cell::RefCell;
use std::thread::JoinHandle;

extern crate alloc;

use edge_executor::{Local, SpawnError, Task};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;
use esp_idf_hal::task::thread_spawn::ThreadSpawnConfiguration;
use ruwm::utils::EventBusReceiver;
use static_cell::StaticCell;

use embedded_graphics::prelude::{Point, RgbColor, Size};
use embedded_graphics::primitives::Rectangle;

use display_interface_spi::SPIInterfaceNoCS;

use embedded_hal::digital::v2::OutputPin as EHOutputPin;

use embedded_svc::http::server::Method;
use embedded_svc::utils::asyncify::Asyncify;
use embedded_svc::wifi::{ClientConfiguration, Configuration};

use esp_idf_hal::executor::EspExecutor;
use esp_idf_hal::gpio::*;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi::*;
use esp_idf_hal::ulp::*;
use esp_idf_hal::{adc::*, delay};

use esp_idf_svc::errors::EspIOError;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http::server::ws::{EspHttpWsAsyncConnection, EspHttpWsProcessor};
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::systime::EspSystemTime;
use esp_idf_svc::timer::EspISRTimerService;
use esp_idf_svc::wifi::{WifiDriver, WifiEvent};

use esp_idf_sys::{esp, EspError};

use edge_frame::assets;

use pulse_counter::PulseCounter;

use ruwm::button::PressedLevel;
use ruwm::mqtt::MessageParser;
use ruwm::pulse_counter::PulseCounter as _;
use ruwm::screen::{CroppedAdaptor, FlushableAdaptor, FlushableDrawTarget};
use ruwm::state::{PostcardSerDe, PostcardStorage};
use ruwm::system::{SlowMem, System};
use ruwm::valve::{self, ValveCommand};

#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

const SSID: &str = env!("RUWM_WIFI_SSID");
const PASS: &str = env!("RUWM_WIFI_PASS");

const ASSETS: assets::serve::Assets = edge_frame::assets!("RUWM_WEB");

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

    let mut valve_power_pin = PinDriver::new(peripherals.pins.gpio10)?.into_output()?;
    let mut valve_open_pin = PinDriver::new(peripherals.pins.gpio12)?.into_output()?;
    let mut valve_close_pin = PinDriver::new(peripherals.pins.gpio13)?.into_output()?;

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

    let nvs_default_partition = EspDefaultNvsPartition::take()?;

    static STORAGE: StaticCell<
        Mutex<StdRawMutex, RefCell<PostcardStorage<500, EspNvs<NvsDefault>>>>,
    > = StaticCell::new();
    let storage = &*STORAGE.init(Mutex::new(RefCell::new(PostcardStorage::<500, _>::new(
        EspNvs::new(nvs_default_partition.clone(), "WM", true)?,
        PostcardSerDe,
    ))));

    static SYSTEM: StaticCell<
        System<
            WS_MAX_CONNECTIONS,
            StdRawMutex,
            PostcardStorage<500, EspNvs<NvsDefault>>,
            EspHttpWsAsyncConnection<()>,
        >,
    > = StaticCell::new();
    let system = &*SYSTEM.init(System::new(unsafe { slow_mem.as_mut().unwrap() }, storage));

    let mut timers = unsafe { EspISRTimerService::new() }?.into_async();

    let mut tasks1 = heapless::Vec::<Task<()>, 16>::new();
    let mut executor1 = EspExecutor::<16, Local>::new();

    executor1
        .spawn_local_collect(system.valve(), &mut tasks1)?
        .spawn_local_collect(
            system.valve_spin(
                timers.timer()?,
                valve_power_pin,
                valve_open_pin,
                valve_close_pin,
            ),
            &mut tasks1,
        )?
        .spawn_local_collect(
            system.wm(
                timers.timer()?,
                PulseCounter::new(UlpDriver::new(peripherals.ulp)?).initialize()?,
            ),
            &mut tasks1,
        )?
        .spawn_local_collect(
            system.battery(
                timers.timer()?,
                AdcDriver::new(peripherals.adc1, &AdcConfig::new().calibration(true))?,
                AdcChannelDriver::<_, Atten0dB<_>>::new(peripherals.pins.gpio33)?,
                PinDriver::new(peripherals.pins.gpio14)?.into_input()?,
            ),
            &mut tasks1,
        )?
        .spawn_local_collect(
            system.button1(
                timers.timer()?,
                subscribe_pin(button1_pin, move || system.button1_signal())?,
                PressedLevel::Low,
            ),
            &mut tasks1,
        )?
        .spawn_local_collect(
            system.button2(
                timers.timer()?,
                subscribe_pin(button2_pin, move || system.button2_signal())?,
                PressedLevel::Low,
            ),
            &mut tasks1,
        )?
        .spawn_local_collect(
            system.button3(
                timers.timer()?,
                subscribe_pin(button3_pin, move || system.button3_signal())?,
                PressedLevel::Low,
            ),
            &mut tasks1,
        )?
        .spawn_local_collect(system.emergency(), &mut tasks1)?
        .spawn_local_collect(
            system.keepalive(timers.timer()?, EspSystemTime),
            &mut tasks1,
        )?;

    let mut sysloop = EspSystemEventLoop::take()?;

    let mut wifi = WifiDriver::new(
        peripherals.modem,
        sysloop.clone(),
        Some(nvs_default_partition),
    )?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        password: PASS.into(),
        ..Default::default()
    }))?;

    let wifi_state_changed_source = sysloop.as_async().subscribe()?;

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

    let ws_processor = Mutex::<StdRawMutex, _>::new(RefCell::new(ws_processor));

    let mut httpd = EspHttpServer::new(&Default::default()).unwrap();

    for asset in &ASSETS {
        httpd.fn_handler(asset.0, Method::Get, move |req| {
            assets::serve::serve(req, &asset)
        })?;
    }

    httpd.ws_handler("/ws", move |connection| {
        ws_processor.lock(|ws_processor| ws_processor.borrow_mut().process(connection))
    })?;

    log::info!("Starting execution");

    let display_backlight = peripherals.pins.gpio4;
    let display_dc = peripherals.pins.gpio16;
    let display_rst = peripherals.pins.gpio23;
    let display_spi = peripherals.spi2;
    let display_sclk = peripherals.pins.gpio18;
    let display_sdo = peripherals.pins.gpio19;
    let display_cs = peripherals.pins.gpio5;

    let execution2 = spawn_executor::<8>(
        b"async-exec-mid\0",
        5000,
        move |executor, tasks| {
            executor
                .spawn_local_collect(
                    system.wm_stats(timers.timer().unwrap(), EspSystemTime),
                    tasks,
                )?
                .spawn_local_collect(system.screen(), tasks)?
                .spawn_local_collect(
                    system.screen_draw(
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
                    ),
                    tasks,
                )?
                .spawn_local_collect(
                    system.wifi(
                        wifi,
                        EventBusReceiver::<_, WifiEvent>::new(wifi_state_changed_source),
                    ),
                    tasks,
                )?
                .spawn_local_collect(system.mqtt_receive(mqtt_conn), tasks)?;

            Ok(())
        },
        move || system.should_quit(),
    );

    let execution3 = spawn_executor::<4>(
        b"async-exec-slow\0",
        10000,
        move |executor, tasks| {
            executor
                .spawn_local_collect(
                    system.mqtt_send::<MQTT_MAX_TOPIC_LEN>(client_id, mqtt_client),
                    tasks,
                )?
                .spawn_local_collect(system.web_accept(ws_acceptor), tasks)?
                .spawn_local_collect(system.web_process::<WS_MAX_FRAME_SIZE>(), tasks)?;

            Ok(())
        },
        move || system.should_quit(),
    );

    executor1.with_context(|exec, ctx| {
        exec.run_tasks(ctx, || system.should_quit(), tasks1);
    });

    log::info!("Execution finished, waiting for 2s to workaround a STD/ESP-IDF pthread (?) bug");

    std::thread::sleep(Duration::from_millis(2000));

    execution2.join().unwrap();
    execution3.join().unwrap();

    log::info!("Finished execution");

    Ok(())
}

fn spawn_executor<'a, const C: usize>(
    thread_name: &'static [u8],
    stack_size: usize,
    spawner: impl FnOnce(
            &mut EspExecutor<'a, C, Local>,
            &mut heapless::Vec<Task<()>, C>,
        ) -> Result<(), InitError>
        + Send
        + 'static,
    run_while: impl Fn() -> bool + Send + 'static,
) -> JoinHandle<()> {
    esp_idf_hal::task::thread_spawn::set_conf(&ThreadSpawnConfiguration {
        name: thread_name,
        stack_size,
        ..Default::default()
    })
    .unwrap();

    std::thread::spawn(move || {
        let (mut executor, tasks) = (move || {
            let mut tasks = heapless::Vec::<Task<()>, C>::new();
            let mut executor = EspExecutor::<C, Local>::new();

            spawner(&mut executor, &mut tasks)?;

            Result::<_, InitError>::Ok((executor, tasks))
        })()
        .unwrap();

        executor.with_context(|exec, ctx| {
            exec.run_tasks(ctx, run_while, tasks);
        });
    })
}

fn subscribe_pin<'d, P: InputPin>(
    pin: impl Peripheral<P = P> + 'd,
    notify: impl Fn() + Send + 'static,
) -> Result<PinDriver<'d, P, Input>, EspError> {
    let mut pin = PinDriver::new(pin)?.into_input()?;

    pin.set_interrupt_type(InterruptType::NegEdge)?;
    unsafe {
        pin.subscribe(notify)?;
    }

    Ok(pin)
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
    power_pin: &mut impl EHOutputPin<Error = impl Debug>,
    open_pin: &mut impl EHOutputPin<Error = impl Debug>,
    close_pin: &mut impl EHOutputPin<Error = impl Debug>,
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

fn display<'d>(
    backlight: impl Peripheral<P = impl OutputPin> + 'd,
    dc: impl Peripheral<P = impl OutputPin> + 'd,
    rst: impl Peripheral<P = impl OutputPin> + 'd,
    spi: impl Peripheral<P = SPI2> + 'd,
    sclk: impl Peripheral<P = impl OutputPin> + 'd,
    sdo: impl Peripheral<P = impl OutputPin> + 'd,
    cs: Option<impl Peripheral<P = impl OutputPin> + 'd>,
) -> Result<impl FlushableDrawTarget<Color = impl RgbColor, Error = impl Debug> + 'd, InitError> {
    let mut backlight = PinDriver::new(backlight)?.into_output()?;

    backlight.set_high()?;

    let di = SPIInterfaceNoCS::new(
        SpiMasterDriver::<SPI2>::new(
            spi,
            sclk,
            sdo,
            Option::<Gpio21>::None,
            cs,
            &SpiMasterConfig::new().baudrate(26.MHz().into()),
        )?,
        PinDriver::new(dc)?.into_output()?,
    );

    let mut display = st7789::ST7789::new(
        di,
        PinDriver::new(rst)?.into_output()?,
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

//impl std::error::Error for InitError {}

pub struct StdRawMutex(std::sync::Mutex<()>);

unsafe impl RawMutex for StdRawMutex {
    const INIT: Self = StdRawMutex(std::sync::Mutex::new(()));

    fn lock<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self.0.lock().unwrap();

        let result = f();

        result
    }
}
