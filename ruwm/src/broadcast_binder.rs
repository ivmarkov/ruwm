use core::fmt::Debug;
use core::future::Future;
use core::time::Duration;

extern crate alloc;
use alloc::string::String;

use alloc::vec::Vec;
use embedded_svc::sys_time::SystemTime;
use embedded_svc::ws;

use embedded_graphics::prelude::RgbColor;

use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_svc::mqtt::client::asyncs::{Client, Connection, Publish};
use embedded_svc::mutex::Mutex;
use embedded_svc::unblocker::asyncs::Unblocker;
use embedded_svc::{
    channel::asyncs::{Receiver, Sender},
    timer::asyncs::TimerService,
    utils::asyncs::channel::adapt,
};

use crate::broadcast_event::WifiStatus;
use crate::pulse_counter::PulseCounter;
use crate::state_snapshot::StateSnapshot;
use crate::storage::Storage;
use crate::web::SenderInfo;
use crate::{
    battery, emergency, error, event_logger, keepalive, mqtt, pipe, quit, valve, water_meter, web,
};
use crate::{
    battery::BatteryState,
    broadcast_event::{BroadcastEvent, Payload},
    button::{self, ButtonId, PressedLevel},
    screen::{self, FlushableDrawTarget},
    valve::ValveState,
    water_meter::WaterMeterState,
};

pub trait SignalFactory<'a> {
    type Sender<D>: Sender<Data = D> + Send
    where
        D: Send + 'a;
    type Receiver<D>: Receiver<Data = D> + Send
    where
        D: Send + 'a;

    fn create<D>(&mut self) -> error::Result<(Self::Sender<D>, Self::Receiver<D>)>
    where
        D: Send + Clone + 'a;
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TaskPriority {
    High,
    Medium,
    Low,
}

pub trait Spawner<'a> {
    fn spawn(
        &mut self,
        priority: TaskPriority,
        fut: impl Future<Output = error::Result<()>> + Send + 'a,
    ) -> error::Result<()>;
}

pub struct BroadcastBinder<U, MV, MW, MB, S, R, T, N, P> {
    unblocker: U,
    bc_sender: S,
    bc_receiver: R,
    timers: T,
    signal_factory: N,
    valve_state: StateSnapshot<MV>,
    water_meter_state: StateSnapshot<MW>,
    battery_state: StateSnapshot<MB>,
    spawner: P,
}

impl<'a, U, MV, MW, MB, S, R, T, N, P> BroadcastBinder<U, MV, MW, MB, S, R, T, N, P>
where
    U: Unblocker + Clone + Send + Sync + 'static,
    MV: Mutex<Data = Option<ValveState>> + Send + Sync + 'static,
    MW: Mutex<Data = WaterMeterState> + Send + Sync + 'static,
    MB: Mutex<Data = BatteryState> + Send + Sync + 'static,
    S: Sender<Data = BroadcastEvent> + Clone + Send + 'static,
    R: Receiver<Data = BroadcastEvent> + Clone + Send + 'static,
    T: TimerService + 'static,
    N: SignalFactory<'static> + 'static,
    P: Spawner<'static> + 'static,
{
    pub fn new(unblocker: U, broadcast: (S, R), timers: T, signal_factory: N, spawner: P) -> Self {
        Self {
            unblocker,
            bc_sender: broadcast.0,
            bc_receiver: broadcast.1,
            timers,
            signal_factory,
            valve_state: StateSnapshot::<MV>::new(),
            water_meter_state: StateSnapshot::<MW>::new(),
            battery_state: StateSnapshot::<MB>::new(),
            spawner,
        }
    }

    pub fn valve_state(&self) -> &StateSnapshot<MV> {
        &self.valve_state
    }

    pub fn water_meter_state(&self) -> &StateSnapshot<MW> {
        &self.water_meter_state
    }

    pub fn battery_state(&self) -> &StateSnapshot<MB> {
        &self.battery_state
    }

    pub fn event_logger(&mut self) -> error::Result<&mut Self> {
        self.spawn(
            TaskPriority::Medium,
            event_logger::run(self.bc_receiver.clone()),
        )
    }

    pub fn emergency(&mut self) -> error::Result<&mut Self> {
        let fut = emergency::run(
            self.sender_signal(TaskPriority::High, |p| {
                Some(BroadcastEvent::new("EMERGENCY", Payload::ValveCommand(p)))
            })?,
            self.receiver_signal_into(TaskPriority::High)?,
            self.receiver_signal_into(TaskPriority::High)?,
            self.receiver_signal_into(TaskPriority::High)?,
        );

        self.spawn(TaskPriority::High, fut)
    }

    pub fn keepalive(
        &mut self,
        system_time: impl SystemTime + Send + 'static,
    ) -> error::Result<&mut Self> {
        let fut = keepalive::run(
            self.bc_receiver.clone(),
            self.timers.timer()?,
            system_time,
            self.sender_signal(TaskPriority::High, |p| {
                Some(BroadcastEvent::new("KEEPALIVE", Payload::RemainingTime(p)))
            })?,
            self.sender_signal(TaskPriority::High, |p| {
                Some(BroadcastEvent::new("KEEPALIVE", Payload::Quit(p)))
            })?,
        );

        self.spawn(TaskPriority::High, fut)
    }

    pub fn wifi(
        &mut self,
        wifi: impl Receiver<Data = impl Send + Sync + Clone + 'static> + Send + 'static,
    ) -> error::Result<&mut Self> {
        let fut = pipe::run(
            wifi,
            self.sender_signal(TaskPriority::Medium, |_| {
                Some(BroadcastEvent::new("WIFI", Payload::WifiStatus(WifiStatus)))
            })?,
        );

        self.spawn(TaskPriority::Medium, fut)
    }

    pub fn web<A, M>(&mut self, web: A) -> error::Result<&mut Self>
    where
        A: ws::asyncs::Acceptor + Send + 'static,
        M: Mutex<Data = Vec<SenderInfo<A>>> + Send + Sync + 'static,
    {
        let sis = web::sis::<A, M>();

        let web_sender = web::run_sender(
            sis.clone(),
            self.adapt_bc_receiver_into(),
            self.receiver_signal_into(TaskPriority::Medium)?,
            self.receiver_signal_into(TaskPriority::Medium)?,
            self.receiver_signal_into(TaskPriority::Medium)?,
        );

        let web_receiver = web::run_receiver(
            sis,
            web,
            self.adapt_bc_sender(|(connection_id, event)| {
                Some(BroadcastEvent::new(
                    "WEB",
                    Payload::WebResponse(connection_id, event),
                ))
            }),
            self.sender_signal(TaskPriority::Medium, |p| {
                Some(BroadcastEvent::new("WEB", Payload::ValveCommand(p)))
            })?,
            self.sender_signal(TaskPriority::Medium, |p| {
                Some(BroadcastEvent::new("WEB", Payload::WaterMeterCommand(p)))
            })?,
            self.valve_state.clone(),
            self.water_meter_state.clone(),
            self.battery_state.clone(),
        );

        self.spawn(TaskPriority::Medium, web_sender)?
            .spawn(TaskPriority::Medium, web_receiver)
    }

    pub fn valve(
        &mut self,
        power_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
        open_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
        close_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
    ) -> error::Result<&mut Self> {
        let (vsc_sender, vsc_receiver) = self.signal_factory.create()?;
        let (vsn_sender, vsn_receiver) = self.signal_factory.create()?;

        let valve_events = valve::run_events(
            self.valve_state.clone(),
            self.receiver_signal_into(TaskPriority::High)?,
            self.sender_signal(TaskPriority::High, |p| {
                Some(BroadcastEvent::new("VALVE", Payload::ValveState(p)))
            })?,
            vsc_sender,
            vsn_receiver,
        );

        let valve_spin = valve::run_spin(
            self.timers.timer()?,
            vsc_receiver,
            vsn_sender,
            power_pin,
            open_pin,
            close_pin,
        );

        self.spawn(TaskPriority::High, valve_events)?
            .spawn(TaskPriority::High, valve_spin)
    }

    pub fn water_meter(
        &mut self,
        pulse_counter: impl PulseCounter + Send + 'static,
    ) -> error::Result<&mut Self> {
        let fut = water_meter::run(
            self.water_meter_state.clone(),
            self.receiver_signal_into(TaskPriority::High)?,
            self.sender_signal(TaskPriority::High, |p| {
                Some(BroadcastEvent::new("WM", Payload::WaterMeterState(p)))
            })?,
            self.timers.timer()?,
            pulse_counter,
        );

        self.spawn(TaskPriority::High, fut)
    }

    pub fn battery<ADC: 'static, BP>(
        &mut self,
        one_shot: impl adc::OneShot<ADC, u16, BP> + Send + 'static,
        battery_pin: BP,
        power_pin: impl InputPin<Error = impl error::HalError + Send + 'static> + Send + 'static,
    ) -> error::Result<&mut Self>
    where
        BP: adc::Channel<ADC> + Send + 'static,
    {
        let fut = battery::run(
            self.battery_state.clone(),
            self.sender_signal(TaskPriority::High, |p| {
                Some(BroadcastEvent::new("BATTERY", Payload::BatteryState(p)))
            })?,
            self.timers.timer()?,
            one_shot,
            battery_pin,
            power_pin,
        );

        self.spawn(TaskPriority::High, fut)
    }

    pub fn mqtt(
        &mut self,
        topic_prefix: impl Into<String>,
        mqtt: (
            impl Client + Publish + Send + 'static,
            impl Connection + Send + 'static,
        ),
    ) -> error::Result<&mut Self> {
        let (mqtt_client, mqtt_connection) = mqtt;

        // TODO: Think what to do with publish notifications as they might block the broadcast queue
        // when it is full

        let mqtt_sender = mqtt::run_sender(
            topic_prefix.into(),
            mqtt_client,
            self.adapt_bc_sender(|p| {
                Some(BroadcastEvent::new(
                    "MQTT",
                    Payload::MqttPublishNotification(p),
                ))
            }),
            self.receiver_signal_into(TaskPriority::Medium)?,
            self.receiver_signal_into(TaskPriority::Medium)?,
            self.receiver_signal_into(TaskPriority::Medium)?,
            self.receiver_signal_into(TaskPriority::Medium)?,
        );

        let mqtt_receiver = mqtt::run_receiver(
            mqtt_connection,
            self.adapt_bc_sender(|p| {
                Some(BroadcastEvent::new(
                    "MQTT",
                    Payload::MqttClientNotification(p),
                ))
            }),
            self.sender_signal(TaskPriority::Medium, |p| {
                Some(BroadcastEvent::new("MQTT", Payload::ValveCommand(p)))
            })?,
            self.sender_signal(TaskPriority::Medium, |p| {
                Some(BroadcastEvent::new("MQTT", Payload::WaterMeterCommand(p)))
            })?,
        );

        self.spawn(TaskPriority::Low, mqtt_sender)?
            .spawn(TaskPriority::Medium, mqtt_receiver)
    }

    pub fn button(
        &mut self,
        id: ButtonId,
        source: &'static str,
        pin: (
            impl Receiver + Send + 'static,
            impl InputPin<Error = impl error::HalError + 'static> + Send + 'static,
        ),
        pressed_level: PressedLevel,
        debounce_time: Option<Duration>,
    ) -> error::Result<&mut Self> {
        let (pin_edge, pin) = pin;

        // TODO: Consider moving the commands to signal_sender for optimization
        // (coalesce multiple commands of the same type)

        let fut = button::run(
            id,
            pin_edge,
            pin,
            self.timers.timer()?,
            self.adapt_bc_sender(move |p| {
                Some(BroadcastEvent::new(source, Payload::ButtonCommand(p)))
            }),
            pressed_level,
            debounce_time,
        );

        self.spawn(TaskPriority::High, fut)
    }

    pub fn screen(
        &mut self,
        display: impl FlushableDrawTarget<Color = impl RgbColor, Error = impl Debug> + Send + 'static,
    ) -> error::Result<&mut Self> {
        let (de_sender, de_receiver) = self.signal_factory.create()?;

        let screen = screen::run_screen(
            self.adapt_bc_receiver_into(),
            self.receiver_signal_into(TaskPriority::Medium)?,
            self.receiver_signal_into(TaskPriority::Medium)?,
            self.receiver_signal_into(TaskPriority::Medium)?,
            self.valve_state.get(),
            self.water_meter_state.get(),
            self.battery_state.get(),
            de_sender,
        );

        let draw_engine = screen::run_draw_engine(self.unblocker.clone(), de_receiver, display);

        self.spawn(TaskPriority::Medium, screen)?
            .spawn(TaskPriority::Low, draw_engine)
    }

    pub fn quit(&mut self) -> error::Result<impl Future<Output = error::Result<()>>> {
        Ok(quit::run(self.adapt_bc_receiver_into()))
    }

    pub fn finish(self) -> error::Result<P> {
        Ok(self.spawner)
    }

    fn adapt_bc_sender<Q>(
        &self,
        adapter: impl Fn(Q) -> Option<BroadcastEvent> + Send + Sync,
    ) -> impl Sender<Data = Q>
    where
        Q: Send,
    {
        adapt::sender(self.bc_sender.clone(), adapter)
    }

    fn adapt_bc_receiver<Q>(
        &self,
        adapter: impl Fn(BroadcastEvent) -> Option<Q> + Send + Sync,
    ) -> impl Receiver<Data = Q>
    where
        Q: Send,
    {
        adapt::receiver(self.bc_receiver.clone(), adapter)
    }

    fn adapt_bc_receiver_into<Q>(&self) -> impl Receiver<Data = Q> + Send
    where
        Q: Send,
        Option<Q>: From<BroadcastEvent>,
    {
        self.adapt_bc_receiver(Into::into)
    }

    fn receiver_signal_into<D>(
        &mut self,
        priority: TaskPriority,
    ) -> error::Result<impl Receiver<Data = D> + 'static>
    where
        D: Send + Sync + Clone + 'static,
        Option<D>: From<BroadcastEvent>,
    {
        self.receiver_signal(priority, Into::into)
    }

    fn receiver_signal<D>(
        &mut self,
        priority: TaskPriority,
        adapter: impl Fn(BroadcastEvent) -> Option<D> + Send + Sync + 'static,
    ) -> error::Result<impl Receiver<Data = D> + 'static>
    where
        D: Send + Sync + Clone + 'static,
    {
        let (signal_sender, signal_receiver) = self.signal_factory.create()?;

        let receiver = self.bc_receiver.clone();

        self.spawn(
            priority,
            pipe::run_transform(receiver, signal_sender, adapter),
        )?;

        Ok(signal_receiver)
    }

    fn sender_signal<D>(
        &mut self,
        priority: TaskPriority,
        adapter: impl Fn(D) -> Option<S::Data> + Send + Sync + 'static,
    ) -> error::Result<impl Sender<Data = D> + 'static>
    where
        D: Send + Sync + Clone + 'static,
    {
        let (signal_sender, signal_receiver) = self.signal_factory.create()?;

        let sender = self.bc_sender.clone();

        self.spawn(
            priority,
            pipe::run_transform(signal_receiver, sender, adapter),
        )?;

        Ok(signal_sender)
    }

    fn spawn(
        &mut self,
        priority: TaskPriority,
        fut: impl Future<Output = error::Result<()>> + Send + 'static,
    ) -> error::Result<&mut Self> {
        self.spawner.spawn(priority, fut)?;

        Ok(self)
    }
}
