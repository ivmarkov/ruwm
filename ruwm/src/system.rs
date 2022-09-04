use core::cell::RefCell;
use core::fmt::Debug;
use core::time::Duration;

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;

use embedded_graphics::prelude::RgbColor;

use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::storage::Storage;
use embedded_svc::sys_time::SystemTime;
use embedded_svc::timer::asynch::OnceTimer;
use embedded_svc::wifi::Wifi as WifiTrait;
use embedded_svc::ws;
use embedded_svc::ws::asynch::server::Acceptor;

use crate::battery::Battery;
use crate::button::{self, PressedLevel};
use crate::channel::Receiver;
use crate::emergency::Emergency;
use crate::keepalive::{Keepalive, RemainingTime};
use crate::mqtt::{Mqtt, MqttCommand};
use crate::notification::Notification;
use crate::pulse_counter::{PulseCounter, PulseWakeup};
use crate::screen::{FlushableDrawTarget, Screen};
use crate::signal::Signal;
use crate::state::NoopStateCell;
use crate::utils::{NotifReceiver, NotifSender, NotifSender2, SignalSender};
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
    storage: &'static Mutex<R, RefCell<S>>,
    valve: Valve<R>,
    wm: WaterMeter<R, S>,
    wm_stats: WaterMeterStats<R>,
    battery: Battery<R>,

    button1: Notification,
    button2: Notification,
    button3: Notification,

    emergency: Emergency,
    keepalive: Keepalive,

    remaining_time: Signal<R, RemainingTime>,

    quit: Notification,

    screen: Screen<R>,

    wifi: Wifi<R>,
    web: Web<N, R, T>,
    mqtt: Mqtt<R>,
}

impl<const N: usize, R, S, T> System<N, R, S, T>
where
    R: RawMutex + Send + Sync + 'static,
    S: Storage + Send + 'static,
    T: ws::asynch::Sender + ws::asynch::Receiver,
{
    pub fn new(slow_mem: &'static mut SlowMem, storage: &'static Mutex<R, RefCell<S>>) -> Self {
        Self {
            storage,
            valve: Valve::new(&mut slow_mem.valve),
            wm: WaterMeter::new(&mut slow_mem.wm, storage),
            wm_stats: WaterMeterStats::new(&mut slow_mem.wm_stats),
            battery: Battery::new(),
            button1: Notification::new(),
            button2: Notification::new(),
            button3: Notification::new(),
            emergency: Emergency::new(),
            keepalive: Keepalive::new(),
            remaining_time: Signal::new(),
            quit: Notification::new(),
            screen: Screen::new(),
            wifi: Wifi::new(),
            web: Web::new(),
            mqtt: Mqtt::new(),
        }
    }

    pub async fn valve(&'static self) {
        self.valve
            .process(NotifSender2::new(
                "VALVE STATE",
                [
                    self.keepalive.event_sink(),
                    self.screen.valve_state_sink(),
                    self.mqtt.valve_state_sink(),
                ],
                self.web.valve_state_sinks(),
            ))
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

    pub async fn wm(
        &'static self,
        pulse_counter: impl PulseCounter,
        pulse_wakeup: impl PulseWakeup,
    ) {
        self.wm
            .process(
                pulse_counter,
                pulse_wakeup,
                NotifSender2::new(
                    "WM STATE",
                    [
                        self.keepalive.event_sink(),
                        self.wm_stats.wm_state_sink(),
                        self.screen.wm_state_sink(),
                        self.mqtt.wm_state_sink(),
                    ],
                    self.web.wm_state_sinks(),
                ),
                NotifSender2::new(
                    "WM STATE",
                    [
                        self.keepalive.event_sink(),
                        self.wm_stats.wm_state_sink(),
                        self.screen.wm_state_sink(),
                        self.mqtt.wm_state_sink(),
                    ],
                    self.web.wm_state_sinks(),
                ),
            )
            .await
    }

    pub async fn wm_stats(&'static self, timer: impl OnceTimer, sys_time: impl SystemTime) {
        self.wm_stats
            .process(
                timer,
                sys_time,
                self.wm.state(),
                NotifSender2::new(
                    "WM STATS STATE",
                    [
                        self.keepalive.event_sink(),
                        self.screen.wm_stats_state_sink(),
                    ],
                    self.web.wm_stats_state_sinks(),
                ),
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
                NotifSender2::new(
                    "BATTERY STATE",
                    [
                        self.keepalive.event_sink(),
                        self.screen.battery_state_sink(),
                        self.mqtt.battery_state_sink(),
                    ],
                    self.web.battery_state_sinks(),
                ),
            )
            .await
    }

    pub fn button1_signal(&self) {
        self.button1.notify();
    }

    pub fn button2_signal(&self) {
        self.button2.notify();
    }

    pub fn button3_signal(&self) {
        self.button3.notify();
    }

    pub async fn button1(
        &'static self,
        timer: impl OnceTimer,
        pin: impl InputPin,
        pressed_level: PressedLevel,
    ) {
        button::process(
            NotifReceiver::new(&self.button1, &NoopStateCell),
            pin,
            pressed_level,
            Some((timer, Duration::from_millis(50))),
            NotifSender::new(
                "BUTTON1 STATE",
                [
                    self.keepalive.event_sink(),
                    self.screen.button1_pressed_sink(),
                ],
            ),
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
            NotifReceiver::new(&self.button2, &NoopStateCell),
            pin,
            pressed_level,
            Some((timer, Duration::from_millis(50))),
            NotifSender::new(
                "BUTTON2 STATE",
                [
                    self.keepalive.event_sink(),
                    self.screen.button2_pressed_sink(),
                ],
            ),
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
            NotifReceiver::new(&self.button3, &NoopStateCell),
            pin,
            pressed_level,
            Some((timer, Duration::from_millis(50))),
            NotifSender::new(
                "BUTTON3 STATE",
                [
                    self.keepalive.event_sink(),
                    self.screen.button3_pressed_sink(),
                ],
            ),
        )
        .await
    }

    pub async fn emergency(&'static self) {
        self.emergency
            .process(
                SignalSender::new("EMERGENCY/VALVE COMMAND", [self.valve.command_sink()]),
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
                SignalSender::new("KEEPALIVE/REMAINING TIME", [&self.remaining_time]), // TODO: Screen
                NotifSender::new("KEEPALIVE/QUIT", [&self.quit]), // TODO: Screen
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
                NotifSender::new("MQTT/SEND", [self.keepalive.event_sink()]),
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
                NotifSender::new("MQTT/RECEIVE", [self.keepalive.event_sink()]),
                SignalSender::new("MQTT/VALVE COMMAND", [self.valve.command_sink()]),
                SignalSender::new("MQTT/WM COMMAND", [self.wm.command_sink()]),
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
                SignalSender::new("WEB/VALVE COMMAND", [self.valve.command_sink()]),
                SignalSender::new("WEB/WM COMMAND", [self.wm.command_sink()]),
                self.valve.state(),
                self.wm.state(),
                self.battery.state(),
            )
            .await
    }

    pub async fn wifi<E>(
        &'static self,
        wifi: impl WifiTrait,
        state_changed_source: impl Receiver<Data = E> + 'static,
    ) {
        self.wifi
            .process(
                wifi,
                state_changed_source,
                NotifSender::new("WIFI", [self.keepalive.event_sink()]),
            )
            .await
    }

    pub fn should_quit(&self) -> bool {
        self.quit.is_triggered()
    }
}
