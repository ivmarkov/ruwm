#![feature(generic_associated_types)]
#![feature(type_alias_impl_trait)]
#![feature(explicit_generic_args_with_impl_trait)]

use core::ops::Deref;
use core::time::Duration;

extern crate alloc;
use alloc::sync::Arc;

use embedded_graphics::prelude::{Point, RgbColor, Size};
use embedded_graphics::primitives::Rectangle;

use display_interface_spi::SPIInterfaceNoCS;

use embedded_hal::digital::v2::OutputPin;

use embedded_svc::event_bus::asyncs::EventBus;
use embedded_svc::executor::asyncs::{Executor, LocalSpawner, Spawner, WaitableExecutor};
use embedded_svc::mutex::MutexFamily;
use embedded_svc::signal::asyncs::Signal;
use embedded_svc::timer::asyncs::TimerService;
use embedded_svc::utils::asyncify::ws::server::AsyncAcceptor;
use embedded_svc::utils::asyncify::Asyncify;
use embedded_svc::utils::asyncs::channel::adapt::both;
use embedded_svc::utils::asyncs::signal::adapt::as_sender;
use embedded_svc::utils::asyncs::signal::{
    adapt as signal_adapt, AtomicSignal, MutexSignal, State,
};
use embedded_svc::utils::atomic_swap::AtomicOption;
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi as WifiTrait};
use embedded_svc::ws::server::registry::Registry;

use esp_idf_hal::gpio::{self, InterruptType, Output, Pull, RTCPin};
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi::SPI2;
use esp_idf_hal::{adc, delay, spi};

use esp_idf_svc::executor::asyncs::{local, sendable};
use esp_idf_svc::http::server::ws::asyncs::EspHttpWsProcessor;
use esp_idf_svc::http::server::ws::EspHttpWsDetachedSender;
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::netif::EspNetifStack;
use esp_idf_svc::nvs::EspDefaultNvs;
use esp_idf_svc::sysloop::EspSysLoopStack;
use esp_idf_svc::systime::EspSystemTime;
use esp_idf_svc::wifi::EspWifi;

use esp_idf_sys::esp;

use edge_frame::assets::serve::*;

use pulse_counter::PulseCounter;

use ruwm::battery::Battery;
use ruwm::button::{self, PressedLevel};
use ruwm::emergency::Emergency;
use ruwm::keepalive::{Keepalive, RemainingTime};
use ruwm::mqtt::{MessageParser, Mqtt};
use ruwm::pulse_counter::PulseCounter as _;
use ruwm::screen::{CroppedAdaptor, FlushableAdaptor, FlushableDrawTarget, Screen};
use ruwm::storage::Storage;
use ruwm::utils::AlmostOnce;
use ruwm::valve;
use ruwm::valve::{Valve, ValveCommand};
use ruwm::water_meter::WaterMeter;
use ruwm::web::Web;
use ruwm::wifi::Wifi;
use ruwm::{checkd, error};
use smol::Task;

use crate::espidf::timer;

mod espidf;

#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

const SSID: &str = env!("RUWM_WIFI_SSID");
const PASS: &str = env!("RUWM_WIFI_PASS");

const ASSETS: Assets = edge_frame::assets!("RUWM_WEB");

const SLEEP_TIME: Duration = Duration::from_secs(30);

type PinSignal = AtomicSignal<AtomicOption, ()>;

const WS_CONNS_MAX: usize = 2;
const WS_FRAME_SIZE: usize = 512;

type MutexFamilyImpl = esp_idf_hal::mutex::Condvar;

static VALVE: AlmostOnce<Valve<MutexFamilyImpl>> = AlmostOnce::new();
static WM: AlmostOnce<WaterMeter<MutexFamilyImpl>> = AlmostOnce::new();
static BATTERY: AlmostOnce<Battery<MutexFamilyImpl>> = AlmostOnce::new();

static BUTTON1: AlmostOnce<PinSignal> = AlmostOnce::new();
static BUTTON2: AlmostOnce<PinSignal> = AlmostOnce::new();
static BUTTON3: AlmostOnce<PinSignal> = AlmostOnce::new();

static EMERGENCY: AlmostOnce<Emergency<MutexFamilyImpl>> = AlmostOnce::new();
static KEEPALIVE: AlmostOnce<Keepalive<MutexFamilyImpl>> = AlmostOnce::new();

static REMAINING_TIME: AlmostOnce<
    MutexSignal<<MutexFamilyImpl as MutexFamily>::Mutex<State<RemainingTime>>, RemainingTime>,
> = AlmostOnce::new();

static QUIT1: AlmostOnce<PinSignal> = AlmostOnce::new();
static QUIT2: AlmostOnce<PinSignal> = AlmostOnce::new();
static QUIT3: AlmostOnce<PinSignal> = AlmostOnce::new();

static SCREEN: AlmostOnce<Screen<MutexFamilyImpl>> = AlmostOnce::new();

static WIFI: AlmostOnce<Wifi<MutexFamilyImpl>> = AlmostOnce::new();
static WEB: AlmostOnce<
    Web<MutexFamilyImpl, AsyncAcceptor<(), MutexFamilyImpl, EspHttpWsDetachedSender>, 4>,
> = AlmostOnce::new();
static MQTT: AlmostOnce<Mqtt<MutexFamilyImpl>> = AlmostOnce::new();

fn main() -> error::Result<()> {
    let wakeup_reason = get_sleep_wakeup_reason()?;

    init()?;

    log::info!("Wakeup reason: {:?}", wakeup_reason);

    error::check!(run(wakeup_reason));

    sleep()?;

    unreachable!()
}

fn run(wakeup_reason: SleepWakeupReason) -> error::Result<()> {
    let peripherals = Peripherals::take().unwrap();

    let mut valve_power_pin = peripherals.pins.gpio10.into_output()?;
    let mut valve_open_pin = peripherals.pins.gpio12.into_output()?;
    let mut valve_close_pin = peripherals.pins.gpio13.into_output()?;

    if wakeup_reason == SleepWakeupReason::ULP {
        emergency_valve_close(
            &mut valve_power_pin,
            &mut valve_open_pin,
            &mut valve_close_pin,
        )?;
    }

    let button1_pin = peripherals.pins.gpio35;
    let button2_pin = peripherals.pins.gpio0;
    let button3_pin = peripherals.pins.gpio27;

    mark_wakeup_pins(&button1_pin, &button2_pin, &button3_pin)?;

    VALVE.init(Valve::new());
    WM.init(WaterMeter::new());
    BATTERY.init(Battery::new());

    BUTTON1.init(PinSignal::new());
    BUTTON2.init(PinSignal::new());
    BUTTON3.init(PinSignal::new());

    EMERGENCY.init(Emergency::new());
    KEEPALIVE.init(Keepalive::new());

    REMAINING_TIME.init(MutexSignal::new());

    QUIT1.init(PinSignal::new());
    QUIT2.init(PinSignal::new());
    QUIT3.init(PinSignal::new());

    SCREEN.init(Screen::new());
    WEB.init(Web::new());
    MQTT.init(Mqtt::new());

    let valve_state_sink = both(MQTT.valve_state_sink(), KEEPALIVE.event_sink());
    let wm_state_sink = both(MQTT.wm_state_sink(), KEEPALIVE.event_sink());
    let battery_state_sink = both(MQTT.battery_state_sink(), KEEPALIVE.event_sink());

    let button1_pressed_sink = both(SCREEN.button1_pressed_sink(), KEEPALIVE.event_sink());
    let button2_pressed_sink = both(SCREEN.button2_pressed_sink(), KEEPALIVE.event_sink());
    let button3_pressed_sink = both(SCREEN.button3_pressed_sink(), KEEPALIVE.event_sink::<()>());

    let wifi_state_sink = KEEPALIVE.event_sink();
    //let web_interest = KEEPALIVE.event();
    //let mqtt_interest = KEEPALIVE.event();

    let quit_sink =
        both(as_sender(QUIT1.deref()), as_sender(QUIT2.deref())).and(as_sender(QUIT3.deref()));

    // Payload::ValveCommand(_)
    // | Payload::ValveState(_)
    // | Payload::WaterMeterCommand(_)
    // | Payload::WaterMeterState(_)
    // | Payload::ButtonCommand(_)
    // | Payload::MqttClientNotification(_)
    // | Payload::WebResponse(_, _) => Some(now + TIMEOUT),
    // Payload::BatteryState(battery_state) => {
    //     battery_state.powered.unwrap_or(true).then(|| now + TIMEOUT)
    // }

    let netif_stack = Arc::new(EspNetifStack::new()?);
    let sysloop_stack = Arc::new(EspSysLoopStack::new()?);
    let nvs_stack = Arc::new(EspDefaultNvs::new()?);

    let mut wifi = EspWifi::new(netif_stack, sysloop_stack, nvs_stack)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        password: PASS.into(),
        ..Default::default()
    }))?;

    let (ws_processor, ws_acceptor) = EspHttpWsProcessor::<WS_CONNS_MAX, WS_FRAME_SIZE>::new(());

    let ws_processor = esp_idf_hal::mutex::Mutex::new(ws_processor);

    let mut httpd = EspHttpServer::new(&Default::default())?;

    register(&mut httpd, &ASSETS)?;

    httpd
        .ws("/ws")
        .handler(move |receiver, sender| ws_processor.lock().process(receiver, sender))?;

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

    let mut timers = timer::timers()?;

    let mut executor1 = local(64);
    let mut executor2 = sendable(64);
    let mut executor3 = sendable(64);

    let mut executor1_tasks = heapless::Vec::<Task<error::Result<()>>, 64>::new();
    let mut executor2_tasks = heapless::Vec::<Task<error::Result<()>>, 64>::new();
    let mut executor3_tasks = heapless::Vec::<Task<error::Result<()>>, 64>::new();

    let mut spawn1 = |fut| {
        executor1_tasks
            .push(executor1.spawn_local(fut)?)
            .map_err(error::heapless)
    };

    spawn1(VALVE.process(valve_state_sink))?;

    executor1_tasks
        .push(executor1.spawn_local(VALVE.spin(
            timers.timer()?,
            valve_power_pin,
            valve_open_pin,
            valve_close_pin,
        ))?)
        .map_err(error::heapless)?;

    executor1_tasks
        .push(executor1.spawn_local(WM.process(
            timers.timer()?,
            PulseCounter::new(peripherals.ulp).initialize()?,
            wm_state_sink,
        ))?)
        .map_err(error::heapless)?;

    executor2_tasks
        .push(executor2.spawn(BATTERY.process(
            timers.timer()?,
            adc::PoweredAdc::new(
                peripherals.adc1,
                adc::config::Config::new().calibration(true),
            )?,
            peripherals.pins.gpio33.into_analog_atten_11db()?,
            peripherals.pins.gpio14.into_input()?,
            battery_state_sink,
        ))?)
        .map_err(error::heapless)?;

    executor1_tasks
        .push(executor1.spawn_local(button::process(
            timers.timer()?,
            signal_adapt::as_receiver(BUTTON1.deref()),
            unsafe { button1_pin.into_subscribed(|| BUTTON1.signal(()), InterruptType::NegEdge)? },
            PressedLevel::Low,
            Some(Duration::from_millis(50)),
            button1_pressed_sink,
        ))?)
        .map_err(error::heapless)?;

    executor2_tasks
        .push(executor2.spawn(button::process(
            timers.timer()?,
            signal_adapt::as_receiver(BUTTON2.deref()),
            unsafe {
                button2_pin
                    .into_subscribed(|| BUTTON2.signal(()), InterruptType::NegEdge)?
                    .into_pull_up()?
            },
            PressedLevel::Low,
            Some(Duration::from_millis(50)),
            button2_pressed_sink,
        ))?)
        .map_err(error::heapless)?;

    executor1_tasks
        .push(executor1.spawn_local(button::process(
            timers.timer()?,
            signal_adapt::as_receiver(BUTTON3.deref()),
            unsafe {
                button3_pin
                    .into_subscribed(|| BUTTON3.signal(()), InterruptType::NegEdge)?
                    .into_pull_up()?
            },
            PressedLevel::Low,
            Some(Duration::from_millis(50)),
            button3_pressed_sink,
        ))?)
        .map_err(error::heapless)?;

    executor2_tasks
        .push(executor2.spawn(EMERGENCY.process(VALVE.command_sink()))?)
        .map_err(error::heapless)?;

    executor2_tasks
        .push(executor2.spawn(KEEPALIVE.process(
            timers.timer()?,
            EspSystemTime,
            as_sender(REMAINING_TIME.deref()),
            quit_sink,
        ))?)
        .map_err(error::heapless)?;

    executor2_tasks
        .push(executor2.spawn(SCREEN.draw(display(
            peripherals.pins.gpio4.into_output()?.degrade(),
            peripherals.pins.gpio16.into_output()?.degrade(),
            peripherals.pins.gpio23.into_output()?.degrade(),
            peripherals.spi2,
            peripherals.pins.gpio18.into_output()?.degrade(),
            peripherals.pins.gpio19.into_output()?.degrade(),
            Some(peripherals.pins.gpio5.into_output()?.degrade()),
        )?))?)
        .map_err(error::heapless)?;

    executor2_tasks
        .push(executor2.spawn(SCREEN.process(
            VALVE.state().get(),
            WM.state().get(),
            BATTERY.state().get(),
        ))?)
        .map_err(error::heapless)?;

    executor3_tasks
        .push(executor3.spawn(MQTT.send(client_id, mqtt_client))?)
        .map_err(error::heapless)?;

    executor3_tasks
        .push(executor3.spawn(MQTT.receive(mqtt_conn, VALVE.command_sink(), WM.command_sink()))?)
        .map_err(error::heapless)?;

    executor3_tasks
        .push(executor3.spawn(WEB.send::<WS_FRAME_SIZE>())?)
        .map_err(error::heapless)?;

    executor3_tasks
        .push(executor3.spawn(WEB.receive::<WS_FRAME_SIZE>(
            ws_acceptor,
            VALVE.state(),
            WM.state(),
            BATTERY.state(),
            VALVE.command_sink(),
            WM.command_sink(),
        ))?)
        .map_err(error::heapless)?;

    let wifi_state_changed_source = wifi.as_async().subscribe()?;

    executor3_tasks
        .push(executor3.spawn(WIFI.process(wifi, wifi_state_changed_source, wifi_state_sink))?)
        .map_err(error::heapless)?;

    log::info!("Starting execution");

    let executor2 = std::thread::spawn(move || {
        executor2.with_context(|exec, ctx| {
            exec.run(ctx, || QUIT2.try_get().is_some(), Some(executor2_tasks));
        });
    });

    let executor3 = std::thread::spawn(move || {
        executor3.with_context(|exec, ctx| {
            exec.run(ctx, || QUIT3.try_get().is_some(), Some(executor3_tasks));
        });
    });

    executor1.with_context(|exec, ctx| {
        exec.run(ctx, || QUIT1.try_get().is_some(), Some(executor1_tasks));
    });

    println!("Execution finished, waiting for 500ms to workaround a STD/ESP-IDF pthread (?) bug");
    std::thread::sleep(Duration::from_millis(500));

    checkd!(executor2.join());
    checkd!(executor3.join());

    log::info!("Finished execution");

    Ok(())
}

fn init() -> error::Result<()> {
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
    power_pin: &mut impl OutputPin<Error = impl error::HalError>,
    open_pin: &mut impl OutputPin<Error = impl error::HalError>,
    close_pin: &mut impl OutputPin<Error = impl error::HalError>,
) -> error::Result<()> {
    log::error!("Start: emergency closing valve due to ULP wakeup...");

    valve::start_spin(Some(ValveCommand::Close), power_pin, open_pin, close_pin)?;
    std::thread::sleep(valve::VALVE_TURN_DELAY);
    valve::start_spin(None, power_pin, open_pin, close_pin)?;

    log::error!("End: emergency closing valve due to ULP wakeup");

    Ok(())
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum SleepWakeupReason {
    Unknown,
    ULP,
    Button,
    Timer,
    Other(u32),
}

fn get_sleep_wakeup_reason() -> error::Result<SleepWakeupReason> {
    Ok(match unsafe { esp_idf_sys::esp_sleep_get_wakeup_cause() } {
        esp_idf_sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_UNDEFINED => SleepWakeupReason::Unknown,
        esp_idf_sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_EXT1 => SleepWakeupReason::Button,
        esp_idf_sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_COCPU => SleepWakeupReason::ULP,
        esp_idf_sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_TIMER => SleepWakeupReason::Timer,
        other => SleepWakeupReason::Other(other),
    })
}

fn mark_wakeup_pins(
    button1_pin: &impl RTCPin,
    button2_pin: &impl RTCPin,
    button3_pin: &impl RTCPin,
) -> error::Result<()> {
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

fn sleep() -> error::Result<()> {
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
) -> error::Result<impl FlushableDrawTarget<Color = impl RgbColor, Error = impl core::fmt::Debug>> {
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
