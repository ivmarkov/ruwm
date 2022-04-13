use core::fmt::Debug;
use core::future::Future;
use core::time::Duration;

extern crate alloc;
use alloc::string::String;

use alloc::vec::Vec;
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
use crate::{battery, emergency, error, event_logger, mqtt, pipe, quit, valve, water_meter, web};
use crate::{
    battery::BatteryState,
    broadcast_event::{BroadcastEvent, Payload},
    button::{self, ButtonId, PressedLevel},
    screen::{self, FlushableDrawTarget},
    valve::ValveState,
    water_meter::WaterMeterState,
};

pub trait SignalFactory<'a> {
    type Sender<D>: Sender<Data = D>
    where
        D: 'a;
    type Receiver<D>: Receiver<Data = D>
    where
        D: 'a;

    fn create<D>(&mut self) -> error::Result<(Self::Sender<D>, Self::Receiver<D>)>
    where
        D: Send + Sync + Clone + 'a;
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
        fut: impl Future<Output = error::Result<()>> + 'a,
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
    U: Unblocker + Clone + 'a,
    MV: Mutex<Data = Option<ValveState>> + Send + Sync + 'a,
    MW: Mutex<Data = WaterMeterState> + Send + Sync + 'a,
    MB: Mutex<Data = BatteryState> + Send + Sync + 'a,
    S: Sender<Data = BroadcastEvent> + Clone + 'a,
    R: Receiver<Data = BroadcastEvent> + Clone + 'a,
    T: TimerService + 'a,
    N: SignalFactory<'a> + 'a,
    P: Spawner<'a> + 'a,
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
        let signal = self.signal(TaskPriority::High, |p| {
            Some(BroadcastEvent::new("EMERGENCY", Payload::ValveCommand(p)))
        })?;

        self.spawn(
            TaskPriority::High,
            emergency::run(
                signal,
                self.adapt_bc_receiver_into(),
                self.adapt_bc_receiver_into(),
            ),
        )
    }

    pub fn wifi(
        &mut self,
        wifi: impl Receiver<Data = impl Send + Sync + Clone + 'a> + 'a,
    ) -> error::Result<&mut Self> {
        let signal = self.signal(TaskPriority::Medium, |_| {
            Some(BroadcastEvent::new("WIFI", Payload::WifiStatus(WifiStatus)))
        })?;

        self.spawn(TaskPriority::Medium, pipe::run(wifi, signal))
    }

    pub fn web<A, M>(&mut self, web: A) -> error::Result<&mut Self>
    where
        A: ws::asyncs::Acceptor + 'a,
        M: Mutex<Data = Vec<SenderInfo<A>>> + 'a,
    {
        let sis = web::sis::<A, M>();

        let web_sender = web::run_sender(
            sis.clone(),
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
        );

        // TODO: Consider moving the commands to signal_sender for optimization
        // (coalesce multiple commands of the same type)

        let web_receiver = web::run_receiver(
            sis,
            web,
            self.adapt_bc_sender(|(connection_id, event)| {
                Some(BroadcastEvent::new(
                    "WEB",
                    Payload::WebResponse(connection_id, event),
                ))
            }),
            self.adapt_bc_sender(|p| Some(BroadcastEvent::new("WEB", Payload::ValveCommand(p)))),
            self.adapt_bc_sender(|p| {
                Some(BroadcastEvent::new("WEB", Payload::WaterMeterCommand(p)))
            }),
            self.valve_state.clone(),
            self.water_meter_state.clone(),
            self.battery_state.clone(),
        );

        self.spawn(TaskPriority::Medium, web_sender)?
            .spawn(TaskPriority::Medium, web_receiver)
    }

    pub fn valve(
        &mut self,
        power_pin: impl OutputPin<Error = impl error::HalError + 'a> + 'a,
        open_pin: impl OutputPin<Error = impl error::HalError + 'a> + 'a,
        close_pin: impl OutputPin<Error = impl error::HalError + 'a> + 'a,
    ) -> error::Result<&mut Self> {
        let (vsc_sender, vsc_receiver) = self.signal_factory.create()?;
        let (vsn_sender, vsn_receiver) = self.signal_factory.create()?;

        let valve_events = valve::run_events(
            self.valve_state.clone(),
            self.adapt_bc_receiver_into(),
            self.signal(TaskPriority::High, |p| {
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
        pulse_counter: impl PulseCounter + 'a,
    ) -> error::Result<&mut Self> {
        let signal = self.signal(TaskPriority::High, |p| {
            Some(BroadcastEvent::new("WM", Payload::WaterMeterState(p)))
        })?;
        let timer = self.timers.timer()?;

        self.spawn(
            TaskPriority::High,
            water_meter::run(
                self.water_meter_state.clone(),
                self.adapt_bc_receiver_into(),
                signal,
                timer,
                pulse_counter,
            ),
        )
    }

    pub fn battery<ADC: 'a, BP>(
        &mut self,
        one_shot: impl adc::OneShot<ADC, u16, BP> + 'a,
        battery_pin: BP,
        power_pin: impl InputPin<Error = impl error::HalError + 'a> + 'a,
    ) -> error::Result<&mut Self>
    where
        BP: adc::Channel<ADC> + 'a,
    {
        // TODO: Consider moving the state to signal_sender for optimization
        // (coalesce multiple states)

        let timer = self.timers.timer()?;

        self.spawn(
            TaskPriority::High,
            battery::run(
                self.battery_state.clone(),
                self.adapt_bc_sender(|p| {
                    Some(BroadcastEvent::new("BATTERY", Payload::BatteryState(p)))
                }),
                timer,
                one_shot,
                battery_pin,
                power_pin,
            ),
        )
    }

    pub fn mqtt(
        &mut self,
        topic_prefix: impl Into<String>,
        mqtt: (impl Client + Publish + 'a, impl Connection + 'a),
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
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
        );

        // TODO: Consider moving the commands to signal_sender for optimization
        // (coalesce multiple commands of the same type)

        let mqtt_receiver = mqtt::run_receiver(
            mqtt_connection,
            self.adapt_bc_sender(|p| {
                Some(BroadcastEvent::new(
                    "MQTT",
                    Payload::MqttClientNotification(p),
                ))
            }),
            self.adapt_bc_sender(|p| Some(BroadcastEvent::new("MQTT", Payload::ValveCommand(p)))),
            self.adapt_bc_sender(|p| {
                Some(BroadcastEvent::new("MQTT", Payload::WaterMeterCommand(p)))
            }),
        );

        self.spawn(TaskPriority::Low, mqtt_sender)?
            .spawn(TaskPriority::Medium, mqtt_receiver)
    }

    pub fn button(
        &mut self,
        id: ButtonId,
        source: &'static str,
        pin: (
            impl Receiver + 'a,
            impl InputPin<Error = impl error::HalError + 'a> + 'a,
        ),
        pressed_level: PressedLevel,
        debounce_time: Option<Duration>,
    ) -> error::Result<&mut Self> {
        let (pin_edge, pin) = pin;

        // TODO: Consider moving the commands to signal_sender for optimization
        // (coalesce multiple commands of the same type)

        let timer = self.timers.timer()?;

        self.spawn(
            TaskPriority::High,
            button::run(
                id,
                pin_edge,
                pin,
                timer,
                self.adapt_bc_sender(move |p| {
                    Some(BroadcastEvent::new(source, Payload::ButtonCommand(p)))
                }),
                pressed_level,
                debounce_time,
            ),
        )
    }

    pub fn screen(
        &mut self,
        display: impl FlushableDrawTarget<Color = impl RgbColor, Error = impl Debug> + Send + 'static,
    ) -> error::Result<&mut Self> {
        let (de_sender, de_receiver) = self.signal_factory.create()?;

        let screen = screen::run_screen(
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
            self.adapt_bc_receiver_into(),
            self.valve_state.get(),
            self.water_meter_state.get(),
            self.battery_state.get(),
            de_sender,
        );

        let draw_engine = screen::run_draw_engine(self.unblocker.clone(), de_receiver, display);

        self.spawn(TaskPriority::Medium, screen)?
            .spawn(TaskPriority::Low, draw_engine)
    }

    pub fn finish(self) -> error::Result<(impl Future<Output = error::Result<()>>, P)> {
        let quit = quit::run(self.adapt_bc_receiver_into());

        Ok((quit, self.spawner))
    }

    fn adapt_bc_sender<Q>(
        &self,
        adapter: impl Fn(Q) -> Option<BroadcastEvent>,
    ) -> impl Sender<Data = Q> {
        adapt::sender(self.bc_sender.clone(), adapter)
    }

    fn adapt_bc_receiver<Q>(
        &self,
        adapter: impl Fn(BroadcastEvent) -> Option<Q>,
    ) -> impl Receiver<Data = Q> {
        adapt::receiver(self.bc_receiver.clone(), adapter)
    }

    fn adapt_bc_receiver_into<Q>(&self) -> impl Receiver<Data = Q>
    where
        Option<Q>: From<BroadcastEvent>,
    {
        self.adapt_bc_receiver(Into::into)
    }

    fn signal<D>(
        &mut self,
        priority: TaskPriority,
        adapter: impl Fn(D) -> Option<S::Data> + 'a,
    ) -> error::Result<impl Sender<Data = D> + 'a>
    where
        D: Send + Sync + Clone + 'a,
    {
        // let signal_sender = adapt::sender(self.bc_sender(), adapter);

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
        fut: impl Future<Output = error::Result<()>> + 'a,
    ) -> error::Result<&mut Self> {
        self.spawner.spawn(priority, fut)?;

        Ok(self)
    }
}
