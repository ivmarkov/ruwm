use core::fmt::Debug;
use core::future::Future;
use core::marker::PhantomData;
use core::time::Duration;

extern crate alloc;
use alloc::sync::Arc;
use alloc::vec::Vec;

use embedded_svc::utils::asyncify::ws::server::AsyncAcceptor;
use embedded_svc::utils::asyncs::forever::Forever;
use embedded_svc::utils::asyncs::signal::adapt::{as_sender, as_receiver};
use esp_idf_hal::mutex::Condvar;
use esp_idf_svc::http::server::ws::{EspHttpWsSender, EspHttpWsDetachedSender};
use heapless;

use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_graphics::prelude::RgbColor;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::errors;
use embedded_svc::executor::asyncs::Spawner;
use embedded_svc::mqtt::client::asyncs::{Client, Connection, Publish};
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::signal::asyncs::{SendSyncSignalFamily, Signal};
use embedded_svc::sys_time::SystemTime;
use embedded_svc::timer::asyncs::{TimerService, OnceTimer};
use embedded_svc::utils::asyncs::channel::adapt;
use embedded_svc::utils::asyncs::signal::{self, MutexSignal, State};
use embedded_svc::ws;

use ruwm::battery::Battery;
use ruwm::emergency::Emergency;
use ruwm::keepalive::Keepalive;
use ruwm::mqtt::{MqttCommand, Mqtt};
use ruwm::pulse_counter::PulseCounter;
use ruwm::screen::Screen;
use ruwm::state_snapshot::StateSnapshot;
use ruwm::storage::Storage;
use ruwm::valve::{ValveCommand, Valve};
use ruwm::water_meter::WaterMeter;
use ruwm::web::{SenderInfo, Web};
use ruwm::{
    battery, emergency, error, event_logger, keepalive, mqtt, pipe, quit, valve, water_meter, web,
};
use ruwm::{
    battery::BatteryState,
    button::{self, PressedLevel},
    screen::{self, FlushableDrawTarget},
    valve::ValveState,
    water_meter::WaterMeterState,
};

type MutexFamilyImpl = esp_idf_hal::mutex::Condvar;

static VALVE: Forever<Valve<MutexFamilyImpl>> = Forever::new();
static WM: Forever<WaterMeter<MutexFamilyImpl>> = Forever::new();
static BATTERY: Forever<Battery<MutexFamilyImpl>> = Forever::new();

static EMERGENCY: Forever<Emergency<MutexFamilyImpl>> = Forever::new();
static KEEPALIVE: Forever<Keepalive<MutexFamilyImpl, 8>> = Forever::new();

static MQTT: Forever<Mqtt<MutexFamilyImpl>> = Forever::new();
static WEB: Forever<Web<MutexFamilyImpl, AsyncAcceptor<(), MutexFamilyImpl, EspHttpWsDetachedSender>, 4>> = Forever::new();

static SCREEN: Forever<Screen<MutexFamilyImpl>> = Forever::new();

// #[derive(Copy, Clone, Eq, PartialEq, Debug)]
// pub enum TaskPriority {
//     High,
//     Medium,
//     Low,
// }

// pub struct BroadcastBinder<'a, 'b, N, PV, PW, PB, S, R, T, P1, P2, P3>
// where
//     PV: Pool,
//     PW: Pool,
//     PB: Pool,
//     P1: Spawner<'a> + 'static,
//     P2: Spawner<'a> + 'static,
//     P3: Spawner<'a> + 'static,
// {
//     _signal_family: PhantomData<N>,
//     valve_state: StateSnapshot<PV>,
//     water_meter_state: StateSnapshot<PW>,
//     battery_state: StateSnapshot<PB>,
//     bc_sender: S,
//     bc_receiver: R,
//     timers: T,
//     spawner1: (&'b mut P1, &'b mut Vec<P1::Task<error::Result<()>>>),
//     spawner2: Option<(&'b mut P2, &'b mut Vec<P2::Task<error::Result<()>>>)>,
//     spawner3: Option<(&'b mut P3, &'b mut Vec<P3::Task<error::Result<()>>>)>,
// }

// impl<'a, 'b, N, PV, PW, PB, MV, MW, MB, S, R, T, P1, P2, P3>
//     BroadcastBinder<'a, 'b, N, PV, PW, PB, S, R, T, P1, P2, P3>
// where
//     N: SendSyncSignalFamily + 'static,
//     PV: Pool<Data = MV> + 'static,
//     PW: Pool<Data = MW> + 'static,
//     PB: Pool<Data = MB> + 'static,
//     MV: Mutex<Data = Option<ValveState>> + Send + Sync + 'static,
//     MW: Mutex<Data = WaterMeterState> + Send + Sync + 'static,
//     MB: Mutex<Data = BatteryState> + Send + Sync + 'static,
//     S: Sender<Data = BroadcastEvent> + Clone + Send + 'static,
//     R: Receiver<Data = BroadcastEvent> + Clone + Send + 'static,
//     T: TimerService + 'static,
//     P1: Spawner<'a> + 'static,
//     P2: Spawner<'a> + 'static,
//     P3: Spawner<'a> + 'static,
// {
//     pub fn new(
//         broadcast: (S, R),
//         timers: T,
//         spawner1: (&'b mut P1, &'b mut Vec<P1::Task<error::Result<()>>>),
//         spawner2: Option<(&'b mut P2, &'b mut Vec<P2::Task<error::Result<()>>>)>,
//         spawner3: Option<(&'b mut P3, &'b mut Vec<P3::Task<error::Result<()>>>)>,
//     ) -> error::Result<Self> {
//         Ok(Self {
//             _signal_family: PhantomData,
//             valve_state: StateSnapshot::<PV>::new(),
//             water_meter_state: StateSnapshot::<PW>::new(),
//             battery_state: StateSnapshot::<PB>::new(),
//             bc_sender: broadcast.0,
//             bc_receiver: broadcast.1,
//             timers,
//             spawner1,
//             spawner2,
//             spawner3,
//         })
//     }

//     pub fn valve_state(&self) -> &StateSnapshot<PV> {
//         &self.valve_state
//     }

//     pub fn water_meter_state(&self) -> &StateSnapshot<PW> {
//         &self.water_meter_state
//     }

//     pub fn battery_state(&self) -> &StateSnapshot<PB> {
//         &self.battery_state
//     }

//     pub fn event_logger(&mut self) -> error::Result<&mut Self> {
//         self.spawn(
//             TaskPriority::Medium,
//             event_logger::run(self.bc_receiver.clone()),
//         )
//     }

//     pub fn emergency(&mut self) -> error::Result<&mut Self> {
//         let fut = emergency::run(
//             self.sender_signal(TaskPriority::High, |p| {
//                 Some(BroadcastEvent::new("EMERGENCY", Payload::ValveCommand(p)))
//             })?,
//             self.receiver_signal_into(TaskPriority::High)?,
//             self.receiver_signal_into(TaskPriority::High)?,
//             self.receiver_signal_into(TaskPriority::High)?,
//         );

//         self.spawn(TaskPriority::High, fut)
//     }

//     pub fn keepalive(
//         &mut self,
//         system_time: impl SystemTime + Send + 'static,
//     ) -> error::Result<&mut Self> {
//         let fut = keepalive::run(
//             self.bc_receiver.clone(),
//             self.timers.timer()?,
//             system_time,
//             self.sender_signal(TaskPriority::High, |p| {
//                 Some(BroadcastEvent::new("KEEPALIVE", Payload::RemainingTime(p)))
//             })?,
//             self.sender_signal(TaskPriority::High, |p| {
//                 Some(BroadcastEvent::new("KEEPALIVE", Payload::Quit(p)))
//             })?,
//         );

//         self.spawn(TaskPriority::High, fut)
//     }

//     pub fn wifi(
//         &mut self,
//         wifi: impl Receiver<Data = impl Send + Sync + Clone + 'static> + Send + 'static,
//     ) -> error::Result<&mut Self> {
//         let fut = pipe::run(
//             wifi,
//             self.sender_signal(TaskPriority::Medium, |_| {
//                 Some(BroadcastEvent::new("WIFI", Payload::WifiStatus(WifiStatus)))
//             })?,
//         );

//         self.spawn(TaskPriority::Medium, fut)
//     }

//     pub fn web<A, P, M, const C: usize, const F: usize>(&mut self, web: A) -> error::Result<&mut Self>
//     where
//         A: ws::asyncs::Acceptor + Send + 'static,
//         P: Pool<Data = M> + 'static,
//         M: Mutex<Data = heapless::Vec<SenderInfo<A>, C>> + Send + Sync + 'static,
//         for<'x> M::Guard<'x>: Send,
//     {
//         let sis = web::sis::<A, P, M, C>()?;

//         let web_sender = web::run_sender::<A, P, M, C, F>(
//             sis.clone(),
//             self.adapt_bc_receiver_into(),
//             self.receiver_signal_into(TaskPriority::Medium)?,
//             self.receiver_signal_into(TaskPriority::Medium)?,
//             self.receiver_signal_into(TaskPriority::Medium)?,
//         );

//         let web_receiver = web::run_receiver::<A, P, M, PV, PW, PB, MV, MW, MB, C, F>(
//             sis,
//             web,
//             self.adapt_bc_sender(|(connection_id, event)| {
//                 Some(BroadcastEvent::new(
//                     "WEB",
//                     Payload::WebResponse(connection_id, event),
//                 ))
//             }),
//             self.sender_signal(TaskPriority::Medium, |p| {
//                 Some(BroadcastEvent::new("WEB", Payload::ValveCommand(p)))
//             })?,
//             self.sender_signal(TaskPriority::Medium, |p| {
//                 Some(BroadcastEvent::new("WEB", Payload::WaterMeterCommand(p)))
//             })?,
//             self.valve_state.clone(),
//             self.water_meter_state.clone(),
//             self.battery_state.clone(),
//         );

//         self.spawn(TaskPriority::Medium, web_sender)?
//             .spawn(TaskPriority::Medium, web_receiver)
//     }

//     pub fn valve(
//         &mut self,
//         power_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
//         open_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
//         close_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
//     ) -> error::Result<&mut Self> {
//         let (vsc_sender, vsc_receiver) = self.signal()?;
//         let (vsn_sender, vsn_receiver) = self.signal()?;

//         let valve_events = valve::run_events(
//             self.valve_state.clone(),
//             self.receiver_signal_into(TaskPriority::High)?,
//             self.sender_signal(TaskPriority::High, |p| {
//                 Some(BroadcastEvent::new("VALVE", Payload::ValveState(p)))
//             })?,
//             vsc_sender,
//             vsn_receiver,
//         );

//         let valve_spin = valve::run_spin(
//             self.timers.timer()?,
//             vsc_receiver,
//             vsn_sender,
//             power_pin,
//             open_pin,
//             close_pin,
//         );

//         self.spawn(TaskPriority::High, valve_events)?
//             .spawn(TaskPriority::High, valve_spin)
//     }

//     pub fn water_meter(
//         &mut self,
//         pulse_counter: impl PulseCounter + Send + 'static,
//     ) -> error::Result<&mut Self> {
//         let fut = water_meter::run(
//             self.water_meter_state.clone(),
//             self.receiver_signal_into(TaskPriority::High)?,
//             self.sender_signal(TaskPriority::High, |p| {
//                 Some(BroadcastEvent::new("WM", Payload::WaterMeterState(p)))
//             })?,
//             self.timers.timer()?,
//             pulse_counter,
//         );

//         self.spawn(TaskPriority::High, fut)
//     }

//     pub fn battery<ADC: 'static, BP>(
//         &mut self,
//         one_shot: impl adc::OneShot<ADC, u16, BP> + Send + 'static,
//         battery_pin: BP,
//         power_pin: impl InputPin<Error = impl error::HalError + Send + 'static> + Send + 'static,
//     ) -> error::Result<&mut Self>
//     where
//         BP: adc::Channel<ADC> + Send + 'static,
//     {
//         let fut = battery::run(
//             self.battery_state.clone(),
//             self.sender_signal(TaskPriority::High, |p| {
//                 Some(BroadcastEvent::new("BATTERY", Payload::BatteryState(p)))
//             })?,
//             self.timers.timer()?,
//             one_shot,
//             battery_pin,
//             power_pin,
//         );

//         self.spawn(TaskPriority::High, fut)
//     }

//     pub fn mqtt(
//         &mut self,
//         topic_prefix: impl AsRef<str> + Send + 'static,
//         mqtt_client: impl Client + Publish + Send + 'static,
//         mqtt_connection: impl Connection<Message = Option<MqttCommand>, Error = impl errors::Error>
//             + Send
//             + 'static,
//     ) -> error::Result<&mut Self> {
//         // TODO: Think what to do with publish notifications as they might block the broadcast queue
//         // when it is full

//         let mqtt_sender = mqtt::run_sender(
//             topic_prefix,
//             mqtt_client,
//             self.adapt_bc_sender(|p| {
//                 Some(BroadcastEvent::new(
//                     "MQTT",
//                     Payload::MqttPublishNotification(p),
//                 ))
//             }),
//             self.receiver_signal_into(TaskPriority::Medium)?,
//             self.receiver_signal_into(TaskPriority::Medium)?,
//             self.receiver_signal_into(TaskPriority::Medium)?,
//             self.receiver_signal_into(TaskPriority::Medium)?,
//         );

//         let mqtt_receiver = mqtt::run_receiver(
//             mqtt_connection,
//             self.adapt_bc_sender(|p| {
//                 Some(BroadcastEvent::new(
//                     "MQTT",
//                     Payload::MqttClientNotification(p),
//                 ))
//             }),
//             self.sender_signal(TaskPriority::Medium, |p| {
//                 Some(BroadcastEvent::new("MQTT", Payload::ValveCommand(p)))
//             })?,
//             self.sender_signal(TaskPriority::Medium, |p| {
//                 Some(BroadcastEvent::new("MQTT", Payload::WaterMeterCommand(p)))
//             })?,
//         );

//         self.spawn(TaskPriority::Low, mqtt_sender)?
//             .spawn(TaskPriority::Medium, mqtt_receiver)
//     }

//     pub fn button(
//         &mut self,
//         id: ButtonId,
//         source: &'static str,
//         pin: (
//             impl Receiver + Send + 'static,
//             impl InputPin<Error = impl error::HalError + 'static> + Send + 'static,
//         ),
//         pressed_level: PressedLevel,
//         debounce_time: Option<Duration>,
//     ) -> error::Result<&mut Self> {
//         let (pin_edge, pin) = pin;

//         // TODO: Consider moving the commands to signal_sender for optimization
//         // (coalesce multiple commands of the same type)

//         let fut = button::run(
//             id,
//             pin_edge,
//             pin,
//             self.timers.timer()?,
//             self.adapt_bc_sender(move |p| {
//                 Some(BroadcastEvent::new(source, Payload::ButtonCommand(p)))
//             }),
//             pressed_level,
//             debounce_time,
//         );

//         self.spawn(TaskPriority::High, fut)
//     }

//     pub fn screen(
//         &mut self,
//         display: impl FlushableDrawTarget<Color = impl RgbColor, Error = impl Debug> + Send + 'static,
//     ) -> error::Result<&mut Self> {
//         let (de_sender, de_receiver) = self.signal()?;

//         let screen = screen::run_screen(
//             self.adapt_bc_receiver_into(),
//             self.receiver_signal_into(TaskPriority::Medium)?,
//             self.receiver_signal_into(TaskPriority::Medium)?,
//             self.receiver_signal_into(TaskPriority::Medium)?,
//             self.valve_state.get(),
//             self.water_meter_state.get(),
//             self.battery_state.get(),
//             de_sender,
//         );

//         let draw_engine = screen::run_draw_engine(de_receiver, display);

//         self.spawn(TaskPriority::Medium, screen)?
//             .spawn(TaskPriority::Low, draw_engine)
//     }

//     pub fn quit(&mut self, priority: TaskPriority) -> error::Result<impl Fn() -> bool + Send> {
//         let signal = Arc::new(N::Signal::<()>::new());

//         {
//             let signal = signal.clone();
//             let quit = quit::run(
//                 self.adapt_bc_receiver_into(),
//                 signal::adapt::into_sender(signal),
//             );

//             self.spawn(priority, quit)?;
//         }

//         Ok(move || signal.try_get().is_some())
//     }

//     fn adapt_bc_sender<D>(
//         &self,
//         adapter: impl Fn(D) -> Option<BroadcastEvent> + Send + Sync,
//     ) -> impl Sender<Data = D>
//     where
//         D: Send,
//     {
//         adapt::sender(self.bc_sender.clone(), adapter)
//     }

//     fn adapt_bc_receiver<D>(
//         &self,
//         adapter: impl Fn(BroadcastEvent) -> Option<D> + Send + Sync,
//     ) -> impl Receiver<Data = D>
//     where
//         D: Send,
//     {
//         adapt::receiver(self.bc_receiver.clone(), adapter)
//     }

//     fn adapt_bc_receiver_into<D>(&self) -> impl Receiver<Data = D> + Send
//     where
//         D: Send,
//         Option<D>: From<BroadcastEvent>,
//     {
//         self.adapt_bc_receiver(Into::into)
//     }

//     fn receiver_signal_into<D>(
//         &mut self,
//         priority: TaskPriority,
//     ) -> error::Result<impl Receiver<Data = D> + 'static>
//     where
//         D: Send + Sync + Clone + 'static,
//         Option<D>: From<BroadcastEvent>,
//     {
//         self.receiver_signal(priority, Into::into)
//     }

//     fn receiver_signal<D>(
//         &mut self,
//         priority: TaskPriority,
//         adapter: impl Fn(BroadcastEvent) -> Option<D> + Send + Sync + 'static,
//     ) -> error::Result<impl Receiver<Data = D> + 'static>
//     where
//         D: Send + Sync + Clone + 'static,
//     {
//         // Ok(self.adapt_bc_receiver(adapter))

//         let (signal_sender, signal_receiver) = self.signal()?;

//         let receiver = self.bc_receiver.clone();

//         self.spawn(
//             priority,
//             pipe::run_transform(receiver, signal_sender, adapter),
//         )?;

//         Ok(signal_receiver)
//     }

//     fn sender_signal<D>(
//         &mut self,
//         priority: TaskPriority,
//         adapter: impl Fn(D) -> Option<S::Data> + Send + Sync + 'static,
//     ) -> error::Result<impl Sender<Data = D> + 'static>
//     where
//         D: Send + Sync + Clone + 'static,
//     {
//         // Ok(self.adapt_bc_sender(adapter))

//         let (signal_sender, signal_receiver) = self.signal()?;

//         let sender = self.bc_sender.clone();

//         self.spawn(
//             priority,
//             pipe::run_transform(signal_receiver, sender, adapter),
//         )?;

//         Ok(signal_sender)
//     }

//     fn signal<D>(
//         &mut self,
//     ) -> error::Result<(
//         impl Sender<Data = D> + 'static,
//         impl Receiver<Data = D> + 'static,
//     )>
//     where
//         D: Send + 'static,
//     {
//         let signal = Arc::new(N::Signal::<D>::new());

//         Ok((
//             signal::adapt::into_sender(signal.clone()),
//             signal::adapt::into_receiver(signal),
//         ))
//     }

//     fn spawn(
//         &mut self,
//         priority: TaskPriority,
//         fut: impl Future<Output = error::Result<()>> + Send + 'static,
//     ) -> error::Result<&mut Self> {
//         match priority {
//             TaskPriority::High => self.spawner1.1.push(self.spawner1.0.spawn(fut)?),
//             TaskPriority::Medium => {
//                 if let Some(spawner2) = self.spawner2.as_mut() {
//                     spawner2.1.push(spawner2.0.spawn(fut)?);
//                 } else {
//                     self.spawn(TaskPriority::High, fut)?;
//                 }
//             }
//             TaskPriority::Low => {
//                 if let Some(spawner3) = self.spawner3.as_mut() {
//                     spawner3.1.push(spawner3.0.spawn(fut)?);
//                 } else {
//                     self.spawn(TaskPriority::Medium, fut)?;
//                 }
//             }
//         }

//         Ok(self)
//     }
// }
