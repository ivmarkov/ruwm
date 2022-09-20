use core::fmt::Debug;
use core::time::Duration;

use embedded_graphics::prelude::RgbColor;

use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_svc::channel::asynch::Receiver;
use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::mutex::RawMutex;
use embedded_svc::signal::asynch::Signal;
use embedded_svc::storage::Storage;
use embedded_svc::sys_time::SystemTime;
use embedded_svc::timer::asynch::OnceTimer;
use embedded_svc::utils::asynch::channel::adapt::{dummy, merge};
use embedded_svc::utils::asynch::signal::{AtomicSignal, MutexSignal};
use embedded_svc::utils::mutex::Mutex;
use embedded_svc::wifi::Wifi as WifiTrait;
use embedded_svc::ws;
use embedded_svc::ws::asynch::server::Acceptor;

use crate::battery::Battery;
use crate::button::{self, PressedLevel};
use crate::emergency::Emergency;
use crate::event_logger;
use crate::keepalive::{Keepalive, RemainingTime};
use crate::mqtt::{Mqtt, MqttCommand};
use crate::pulse_counter::PulseCounter;
use crate::screen::{FlushableDrawTarget, Screen, Q};
use crate::utils::{as_static_receiver, as_static_sender};
use crate::valve::{Valve, ValveState};
use crate::water_meter::{WaterMeter, WaterMeterState};
use crate::water_meter_stats::{WaterMeterStats, WaterMeterStatsState};
use crate::web::Web;
use crate::wifi::Wifi;

#[derive(Default)]
pub struct SlowMem {
    valve: Option<ValveState>,
    wm: Option<WaterMeterState>, // Only a cache for NVS
    wm_stats: WaterMeterStatsState,
}

pub struct System<const N: usize, R, S, T>
where
    R: RawMutex + 'static,
    S: Storage + Send + 'static,
{
    storage: &'static Mutex<R, S>,
    valve: Valve<R>,
    wm: WaterMeter<R, S>,
    wm_stats: WaterMeterStats<R>,
    battery: Battery<R>,

    button1: AtomicSignal<()>,
    button2: AtomicSignal<()>,
    button3: AtomicSignal<()>,

    emergency: Emergency,
    keepalive: Keepalive,

    remaining_time: MutexSignal<R, RemainingTime>,

    quit: AtomicSignal<()>,

    screen: Screen<R>,

    wifi: Wifi<R>,
    web: Web<N, R, T>,
    mqtt: Mqtt,
}

impl<const N: usize, R, S, T> System<N, R, S, T>
where
    R: RawMutex + 'static,
    S: Storage + Send + 'static,
    T: ws::asynch::Sender + ws::asynch::Receiver,
{
    pub fn new(slow_mem: &'static mut SlowMem, storage: &'static Mutex<R, S>) -> Self {
        Self {
            storage,
            valve: Valve::new(&mut slow_mem.valve),
            wm: WaterMeter::new(&mut slow_mem.wm, storage),
            wm_stats: WaterMeterStats::new(&mut slow_mem.wm_stats),
            battery: Battery::new(),
            button1: AtomicSignal::new(),
            button2: AtomicSignal::new(),
            button3: AtomicSignal::new(),
            emergency: Emergency::new(),
            keepalive: Keepalive::new(),
            remaining_time: MutexSignal::new(),
            quit: AtomicSignal::new(),
            screen: Screen::new(),
            wifi: Wifi::new(),
            web: Web::new(),
            mqtt: Mqtt::new(),
        }
    }

    pub async fn valve(&'static self) {
        self.valve
            .process(
                merge(self.keepalive.event_sink(), event_logger::sink("VALVE"))
                    .and(self.screen.valve_state_sink())
                    .and(self.web.valve_state_sink())
                    .and(self.mqtt.valve_state_sink()),
            )
            .await
    }

    pub async fn valve_spin(
        &'static self,
        once: impl OnceTimer,
        power_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
        open_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
        close_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
    ) {
        self.valve.spin(once, power_pin, open_pin, close_pin).await
    }

    pub async fn wm(&'static self, timer: impl OnceTimer, pulse_counter: impl PulseCounter) {
        self.wm
            .process(
                timer,
                pulse_counter,
                merge(self.keepalive.event_sink(), event_logger::sink("WM"))
                    .and(self.screen.wm_state_sink())
                    .and(self.web.wm_state_sink())
                    .and(self.mqtt.wm_state_sink()),
            )
            .await
    }

    pub async fn wm_stats(&'static self, timer: impl OnceTimer, sys_time: impl SystemTime) {
        self.wm_stats
            .process(
                timer,
                sys_time,
                merge(self.keepalive.event_sink(), event_logger::sink("WM_STATS"))
                    .and(self.screen.wm_stats_state_sink())
                    .and(self.web.wm_stats_state_sink()),
            )
            .await
    }

    pub async fn battery<ADC, BP>(
        &'static self,
        timer: impl OnceTimer,
        one_shot: impl adc::OneShot<ADC, u16, BP>,
        battery_pin: BP,
        power_pin: impl InputPin,
    ) where
        BP: adc::Channel<ADC>,
    {
        self.battery
            .process(
                timer,
                one_shot,
                battery_pin,
                power_pin,
                merge(self.keepalive.event_sink(), event_logger::sink("BATTERY"))
                    .and(self.screen.battery_state_sink())
                    .and(self.web.battery_state_sink())
                    .and(self.mqtt.battery_state_sink()),
            )
            .await
    }

    pub fn button1_signal(&self) {
        self.button1.signal(())
    }

    pub fn button2_signal(&self) {
        self.button2.signal(())
    }

    pub fn button3_signal(&self) {
        self.button3.signal(())
    }

    pub async fn button1(
        &'static self,
        timer: impl OnceTimer,
        pin: impl InputPin,
        pressed_level: PressedLevel,
    ) {
        button::process(
            timer,
            as_static_receiver(&self.button1),
            pin,
            pressed_level,
            Some(Duration::from_millis(50)),
            merge(self.keepalive.event_sink(), event_logger::sink("BUTTON1"))
                .and(self.screen.button1_pressed_sink()),
        )
        .await
    }

    pub async fn button2(
        &'static self,
        timer: impl OnceTimer,
        pin: impl InputPin,
        pressed_level: PressedLevel,
    ) {
        button::process(
            timer,
            as_static_receiver(&self.button2),
            pin,
            pressed_level,
            Some(Duration::from_millis(50)),
            merge(self.keepalive.event_sink(), event_logger::sink("BUTTON2"))
                .and(self.screen.button2_pressed_sink()),
        )
        .await
    }

    pub async fn button3(
        &'static self,
        timer: impl OnceTimer,
        pin: impl InputPin,
        pressed_level: PressedLevel,
    ) {
        button::process(
            timer,
            as_static_receiver(&self.button3),
            pin,
            pressed_level,
            Some(Duration::from_millis(50)),
            merge(self.keepalive.event_sink(), event_logger::sink("BUTTON3"))
                .and(self.screen.button3_pressed_sink()),
        )
        .await
    }

    pub async fn emergency(&'static self) {
        self.emergency
            .process(
                self.valve.command_sink(),
                self.valve.state(),
                self.wm.state(),
                self.battery.state(),
            )
            .await // TODO: Screen
    }

    pub async fn keepalive(&'static self, timer: impl OnceTimer, system_time: impl SystemTime) {
        self.keepalive
            .process(
                timer,
                system_time,
                merge(
                    as_static_sender(&self.remaining_time),
                    event_logger::sink("KEEPALIVE/REMAINING TIME"),
                ), // TODO: Screen
                merge(
                    as_static_sender(&self.quit),
                    event_logger::sink("KEEPALIVE/QUIT"),
                ), // TODO: Screen
            )
            .await
    }

    pub async fn screen_draw<D>(&'static self, display: D)
    where
        D: FlushableDrawTarget + Send + 'static,
        D::Color: RgbColor,
        D::Error: Debug,
    {
        self.screen.draw(display).await
    }

    pub async fn screen(&'static self) {
        self.screen
            .process(
                self.valve.state(),
                self.wm.state(),
                self.wm_stats.state(),
                self.battery.state(),
                event_logger::sink("SCREEN"),
            )
            .await
    }

    pub async fn mqtt_send<const L: usize>(
        &'static self,
        topic_prefix: impl AsRef<str>,
        mqtt: impl Client + Publish,
    ) {
        self.mqtt
            .send::<L>(
                topic_prefix,
                mqtt,
                self.valve.state(),
                self.wm.state(),
                self.battery.state(),
                merge(self.keepalive.event_sink(), event_logger::sink("MQTT/SEND")),
            )
            .await
    }

    pub async fn mqtt_receive(
        &'static self,
        connection: impl Connection<Message = Option<MqttCommand>>,
    ) {
        self.mqtt
            .receive(
                connection,
                dummy(),
                merge(
                    self.keepalive.event_sink(),
                    event_logger::sink("MQTT/RECEIVE"),
                ),
                self.valve.command_sink(),
                self.wm.command_sink(),
            )
            .await
    }

    pub async fn web_accept<A>(&'static self, acceptor: A)
    where
        A: Acceptor<Connection = T>,
    {
        loop {
            let connection = acceptor.accept().await.unwrap();

            self.web.handle(connection).await;
        }
    }

    pub async fn web_accept_handle(&'static self, connection: T) {
        self.web.handle(connection).await;
    }

    pub async fn web_process<const F: usize>(&'static self) {
        self.web
            .process::<F>(
                self.valve.command_sink(),
                self.wm.command_sink(),
                self.valve.state(),
                self.wm.state(),
                self.battery.state(),
            )
            .await
    }

    pub async fn wifi(
        &'static self,
        wifi: impl WifiTrait,
        state_changed_source: impl Receiver<Data = ()> + 'static,
    ) {
        self.wifi
            .process(
                wifi,
                state_changed_source,
                merge(self.keepalive.event_sink(), event_logger::sink("WIFI")),
            )
            .await
    }

    pub fn should_quit(&self) -> bool {
        self.quit.is_set()
    }
}
