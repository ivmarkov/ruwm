use core::cell::RefCell;
use core::fmt::Debug;

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::signal::Signal;

use embassy_time::Duration;
use embedded_graphics::prelude::RgbColor;

use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_svc::mqtt::client::asynch::{Client, Connection, Publish};
use embedded_svc::storage::Storage;
use embedded_svc::wifi::Wifi as WifiTrait;
use embedded_svc::ws;
use embedded_svc::ws::asynch::server::Acceptor;

#[cfg(feature = "edge-executor")]
use edge_executor::*;

use crate::battery::Battery;
use crate::button::{self, PressedLevel};
use crate::channel::{LogSender, NotifSender, Receiver};
use crate::emergency::Emergency;
use crate::keepalive::{Keepalive, RemainingTime};
use crate::mqtt::{Mqtt, MqttCommand};
use crate::notification::Notification;
use crate::pulse_counter::{PulseCounter, PulseWakeup};
use crate::screen::{FlushableDrawTarget, Screen};
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
            .process((
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
        power_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
        open_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
        close_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
    ) {
        self.valve.spin(power_pin, open_pin, close_pin).await
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
                (
                    [
                        self.keepalive.event_sink(),
                        self.wm_stats.wm_state_sink(),
                        self.screen.wm_state_sink(),
                        self.mqtt.wm_state_sink(),
                    ],
                    self.web.wm_state_sinks(),
                ),
                (
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

    pub async fn wm_stats(&'static self) {
        self.wm_stats
            .process(
                self.wm.state(),
                (
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
        one_shot: impl adc::OneShot<ADC, u16, BP>,
        battery_pin: BP,
        power_pin: impl InputPin,
    ) where
        BP: adc::Channel<ADC>,
    {
        self.battery
            .process(
                one_shot,
                battery_pin,
                power_pin,
                (
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

    pub async fn button1(&'static self, pin: impl InputPin, pressed_level: PressedLevel) {
        button::process(
            &self.button1,
            pin,
            pressed_level,
            Some(Duration::from_millis(50)),
            (
                LogSender::new("BUTTON1 STATE"),
                [
                    self.keepalive.event_sink(),
                    self.screen.button1_pressed_sink(),
                ],
            ),
        )
        .await
    }

    pub async fn button2(&'static self, pin: impl InputPin, pressed_level: PressedLevel) {
        button::process(
            &self.button2,
            pin,
            pressed_level,
            Some(Duration::from_millis(50)),
            (
                LogSender::new("BUTTON2 STATE"),
                [
                    self.keepalive.event_sink(),
                    self.screen.button2_pressed_sink(),
                ],
            ),
        )
        .await
    }

    pub async fn button3(&'static self, pin: impl InputPin, pressed_level: PressedLevel) {
        button::process(
            &self.button3,
            pin,
            pressed_level,
            Some(Duration::from_millis(50)),
            (
                LogSender::new("BUTTON3 STATE"),
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
                (
                    LogSender::new("EMERGENCY/VALVE COMMAND"),
                    self.valve.command_sink(),
                ),
                self.valve.state(),
                self.wm.state(),
                self.battery.state(),
            )
            .await // TODO: Screen
    }

    pub async fn keepalive(&'static self) {
        self.keepalive
            .process(
                (
                    LogSender::new("KEEPALIVE/REMAINING TIME"),
                    &self.remaining_time,
                ), // TODO: Screen
                (LogSender::new("KEEPALIVE/QUIT"), &self.quit), // TODO: Screen
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
                (
                    LogSender::new("MQTT/SEND"),
                    NotifSender::from(self.keepalive.event_sink()),
                ),
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
                (
                    LogSender::new("MQTT/RECEIVE"),
                    NotifSender::from(self.keepalive.event_sink()),
                ),
                (
                    LogSender::new("MQTT/VALVE COMMAND"),
                    self.valve.command_sink(),
                ),
                (LogSender::new("MQTT/WM COMMAND"), self.wm.command_sink()),
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

    pub async fn web_process(&'static self) {
        self.web
            .process(
                (
                    LogSender::new("WEB/VALVE COMMAND"),
                    self.valve.command_sink(),
                ),
                (LogSender::new("WEB/WM COMMAND"), self.wm.command_sink()),
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
                (
                    LogSender::new("WIFI"),
                    NotifSender::from(self.keepalive.event_sink()),
                ),
            )
            .await
    }

    pub fn should_quit(&self) -> bool {
        self.quit.is_triggered()
    }

    #[cfg(feature = "edge-executor")]
    pub fn spawn_executor0<EN, EW, ADC, BP>(
        &'static self,
        valve_power_pin: impl OutputPin<Error = impl Debug + 'static> + Send + 'static,
        valve_open_pin: impl OutputPin<Error = impl Debug + 'static> + Send + 'static,
        valve_close_pin: impl OutputPin<Error = impl Debug + 'static> + Send + 'static,
        pulse_counter: impl PulseCounter + 'static,
        pulse_wakeup: impl PulseWakeup + 'static,
        battery_voltage: impl adc::OneShot<ADC, u16, BP> + 'static,
        battery_pin: BP,
        power_pin: impl InputPin + 'static,
        button1_pin: impl InputPin + 'static,
        button2_pin: impl InputPin + 'static,
        button3_pin: impl InputPin + 'static,
    ) -> Result<(Executor<16, EN, EW, Local>, heapless::Vec<Task<()>, 16>), SpawnError>
    where
        EN: NotifyFactory + RunContextFactory + Default,
        EW: Default,
        ADC: 'static,
        BP: adc::Channel<ADC> + 'static,
    {
        let mut executor = Executor::<16, EN, EW, Local>::new();
        let mut tasks = heapless::Vec::<Task<()>, 16>::new();

        executor
            .spawn_local_collect(self.valve(), &mut tasks)?
            .spawn_local_collect(
                self.valve_spin(valve_power_pin, valve_open_pin, valve_close_pin),
                &mut tasks,
            )?
            .spawn_local_collect(self.wm(pulse_counter, pulse_wakeup), &mut tasks)?
            .spawn_local_collect(
                self.battery(battery_voltage, battery_pin, power_pin),
                &mut tasks,
            )?
            .spawn_local_collect(self.button1(button1_pin, PressedLevel::Low), &mut tasks)?
            .spawn_local_collect(self.button2(button2_pin, PressedLevel::Low), &mut tasks)?
            .spawn_local_collect(self.button3(button3_pin, PressedLevel::Low), &mut tasks)?
            .spawn_local_collect(self.emergency(), &mut tasks)?
            .spawn_local_collect(self.keepalive(), &mut tasks)?;

        Ok((executor, tasks))
    }

    #[cfg(feature = "edge-executor")]
    pub fn spawn_executor1<EN, EW, D, E>(
        &'static self,
        display: D,
        wifi: impl WifiTrait + 'static,
        wifi_notif: impl Receiver<Data = E> + 'static,
        mqtt_conn: impl Connection<Message = Option<MqttCommand>> + 'static,
    ) -> Result<(Executor<8, EN, EW, Local>, heapless::Vec<Task<()>, 8>), SpawnError>
    where
        EN: NotifyFactory + RunContextFactory + Default,
        EW: Default,
        D: FlushableDrawTarget + Send + 'static,
        D::Color: RgbColor,
        D::Error: Debug,
        E: 'static,
    {
        let mut executor = Executor::<8, EN, EW, Local>::new();
        let mut tasks = heapless::Vec::<Task<()>, 8>::new();

        executor
            .spawn_local_collect(self.wm_stats(), &mut tasks)?
            .spawn_local_collect(self.screen(), &mut tasks)?
            .spawn_local_collect(self.screen_draw(display), &mut tasks)?
            .spawn_local_collect(self.wifi(wifi, wifi_notif), &mut tasks)?
            .spawn_local_collect(self.mqtt_receive(mqtt_conn), &mut tasks)?;

        Ok((executor, tasks))
    }

    pub fn spawn_executor2<const L: usize, EN, EW>(
        &'static self,
        mqtt_topic_prefix: &'static str,
        mqtt_client: impl Client + Publish + 'static,
        ws_acceptor: impl Acceptor<Connection = T> + 'static,
    ) -> Result<(Executor<4, EN, EW, Local>, heapless::Vec<Task<()>, 4>), SpawnError>
    where
        EN: NotifyFactory + RunContextFactory + Default,
        EW: Default,
    {
        let mut executor = Executor::<4, EN, EW, Local>::new();
        let mut tasks = heapless::Vec::<Task<()>, 4>::new();

        executor
            .spawn_local_collect(
                self.mqtt_send::<L>(mqtt_topic_prefix, mqtt_client),
                &mut tasks,
            )?
            //.spawn_local_collect(self.0.web_accept(ws_acceptor), &mut tasks)?
            //.spawn_local_collect(self.0.web_process(), &mut tasks)?
            ;

        Ok((executor, tasks))
    }

    #[cfg(feature = "edge-executor")]
    pub fn run<const C: usize, EN, EW>(
        &self,
        executor: &mut Executor<C, EN, EW, Local>,
        tasks: heapless::Vec<Task<()>, C>,
    ) where
        EN: NotifyFactory + RunContextFactory + Default,
        EW: Wait + Default,
    {
        executor.with_context(|exec, ctx| {
            exec.run_tasks(ctx, move || !self.should_quit(), tasks);
        });
    }

    #[cfg(all(feature = "std", feature = "edge-executor"))]
    pub fn schedule<'a, const C: usize, EN, EW>(
        &'static self,
        spawner: impl FnOnce() -> Result<
                (Executor<'a, C, EN, EW, Local>, heapless::Vec<Task<()>, C>),
                SpawnError,
            > + Send
            + 'static,
    ) -> std::thread::JoinHandle<()>
    where
        EN: NotifyFactory + RunContextFactory + Default,
        EW: Wait + Default,
        T: Send + 'static,
    {
        std::thread::spawn(move || {
            let (mut executor, tasks) = spawner().unwrap();

            // info!(
            //     "Tasks on thread {:?} scheduled, about to run the executor now",
            //     "TODO"
            // );

            self.run(&mut executor, tasks);
        })
    }
}
