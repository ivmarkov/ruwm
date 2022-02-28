use core::fmt::Display;
use core::future::{ready, Future, Ready};
use core::pin::Pin;
use core::{fmt::Debug, marker::PhantomData};

extern crate alloc;
use alloc::boxed::Box;
use alloc::string::String;

use embedded_graphics::prelude::RgbColor;

use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_svc::mqtt::client::nonblocking::{Client, Connection, Publish};
use embedded_svc::mutex::Mutex;
use embedded_svc::unblocker::nonblocking::Unblocker;
use embedded_svc::{
    channel::nonblocking::{Receiver, Sender},
    timer::nonblocking::TimerService,
    utils::nonblocking::channel::adapt,
};
use futures::future::try_join;
use futures::FutureExt;

use crate::pulse_counter::PulseCounter;
use crate::state_snapshot::StateSnapshot;
use crate::storage::Storage;
use crate::{battery, emergency, event_logger, mqtt, pipe, valve, water_meter};
use crate::{
    battery::BatteryState,
    broadcast_event::{BroadcastEvent, Payload},
    button::{self, ButtonId, PressedLevel},
    screen::{self, FlushableDrawTarget},
    valve::ValveState,
    water_meter::WaterMeterState,
};

pub trait SignalFactory {
    type Sender<D>: Sender<Data = D>;
    type Receiver<D>: Receiver<Data = D>;

    fn create<D>(&mut self) -> anyhow::Result<(Self::Sender<D>, Self::Receiver<D>)>
    where
        D: Send + Sync + Clone + 'static;
}

pub struct BroadcastBinder<U, MV, MW, MB, S, R, T, N, F> {
    _unblocker: PhantomData<U>,
    bc_sender: S,
    bc_receiver: R,
    timers: T,
    signal_factory: N,
    valve_state: StateSnapshot<MV>,
    water_meter_state: StateSnapshot<MW>,
    battery_state: StateSnapshot<MB>,
    joined_fut: F,
}

impl<U, MV, MW, MB, S, R, T, N>
    BroadcastBinder<U, MV, MW, MB, S, R, T, N, Ready<anyhow::Result<()>>>
where
    U: Unblocker,
    MV: Mutex<Data = Option<ValveState>> + Send + Sync,
    MW: Mutex<Data = WaterMeterState> + Send + Sync,
    MB: Mutex<Data = BatteryState> + Send + Sync,
    S: Sender<Data = BroadcastEvent> + Clone,
    R: Receiver<Data = BroadcastEvent> + Clone,
    T: TimerService,
    N: SignalFactory,
{
    pub fn new(broadcast: (S, R), timers: T, signal_factory: N) -> Self {
        Self {
            _unblocker: PhantomData,
            bc_sender: broadcast.0,
            bc_receiver: broadcast.1,
            timers,
            signal_factory,
            valve_state: StateSnapshot::<MV>::new(),
            water_meter_state: StateSnapshot::<MW>::new(),
            battery_state: StateSnapshot::<MB>::new(),
            joined_fut: ready(Ok(())),
        }
    }
}

impl<U, MV, MW, MB, S, R, T, N, F> BroadcastBinder<U, MV, MW, MB, S, R, T, N, F>
where
    U: Unblocker + 'static,
    MV: Mutex<Data = Option<ValveState>> + Send + Sync + 'static,
    MW: Mutex<Data = WaterMeterState> + Send + Sync + 'static,
    MB: Mutex<Data = BatteryState> + Send + Sync + 'static,
    S: Sender<Data = BroadcastEvent> + Clone + 'static,
    R: Receiver<Data = BroadcastEvent> + Clone + 'static,
    T: TimerService + 'static,
    N: SignalFactory + 'static,
    F: Future<Output = anyhow::Result<()>> + 'static,
    Self: Sized,
{
    pub fn valve_state(&self) -> &StateSnapshot<MV> {
        &self.valve_state
    }

    pub fn water_meter_state(&self) -> &StateSnapshot<MW> {
        &self.water_meter_state
    }

    pub fn battery_state(&self) -> &StateSnapshot<MB> {
        &self.battery_state
    }

    pub fn bc_sender(&self) -> &impl Sender<Data = BroadcastEvent> {
        &self.bc_sender
    }

    pub fn bc_receiver(&self) -> &impl Receiver<Data = BroadcastEvent> {
        &self.bc_receiver
    }

    pub fn event_logger(
        self,
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    > {
        let bc_receiver = self.bc_receiver.clone();

        self.bind(event_logger::run(bc_receiver))
    }

    pub fn emergency(
        self,
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    > {
        let bc_sender = self.bc_sender.clone();
        let bc_receiver = self.bc_receiver.clone();

        self.bind(emergency::run(
            adapt::sender(bc_sender, |p| {
                Some(BroadcastEvent::new("EMERGENCY", Payload::ValveCommand(p)))
            }),
            adapt::receiver(bc_receiver.clone(), Into::into),
            adapt::receiver(bc_receiver, Into::into),
        ))
    }

    pub fn wifi(
        self,
        wifi: impl Receiver + 'static,
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    > {
        let bc_sender = self.bc_sender.clone();

        self.bind(pipe::run(
            wifi,
            adapt::sender(bc_sender, |_| {
                Some(BroadcastEvent::new("WIFI", Payload::WifiStatus))
            }),
        ))
    }

    pub fn valve(
        mut self,
        power_pin: impl OutputPin<Error = impl Debug + 'static> + 'static,
        open_pin: impl OutputPin<Error = impl Debug + 'static> + 'static,
        close_pin: impl OutputPin<Error = impl Debug + 'static> + 'static,
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    > {
        let (vsc_sender, vsc_receiver) = self.signal_factory.create()?;
        let (vsn_sender, vsn_receiver) = self.signal_factory.create()?;

        let valve_events = valve::run_events(
            self.valve_state.clone(),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("VALVE", Payload::ValveState(p)))
            }),
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

        self.bind(async move {
            try_join(valve_events, valve_spin).await?;

            Ok(())
        })
    }

    pub fn water_meter(
        mut self,
        pulse_counter: impl PulseCounter + 'static,
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    > {
        let timer = self.timers.timer()?;
        let bc_sender = self.bc_sender.clone();
        let bc_receiver = self.bc_receiver.clone();
        let water_meter_state = self.water_meter_state.clone();

        self.bind(water_meter::run(
            water_meter_state,
            adapt::receiver(bc_receiver, Into::into),
            adapt::sender(bc_sender, |p| {
                Some(BroadcastEvent::new("WM", Payload::WaterMeterState(p)))
            }),
            timer,
            pulse_counter,
        ))
    }

    pub fn battery<ADC: 'static, BP>(
        mut self,
        one_shot: impl adc::OneShot<ADC, u16, BP> + 'static,
        battery_pin: BP,
        power_pin: impl InputPin<Error = impl Debug + 'static> + 'static,
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    >
    where
        BP: adc::Channel<ADC> + 'static,
    {
        let timer = self.timers.timer()?;
        let bc_sender = self.bc_sender.clone();
        let battery_state = self.battery_state.clone();

        self.bind(battery::run(
            battery_state,
            adapt::sender(bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("BATTERY", Payload::BatteryState(p)))
            }),
            timer,
            one_shot,
            battery_pin,
            power_pin,
        ))
    }

    pub fn mqtt(
        self,
        topic_prefix: impl Into<String>,
        mqtt: (
            impl Client + Publish + 'static,
            impl Connection<Error = impl Display + 'static> + 'static,
        ),
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    > {
        let (mqtt_client, mqtt_connection) = mqtt;

        let mqtt_sender = mqtt::run_sender(
            topic_prefix.into(),
            mqtt_client,
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new(
                    "MQTT",
                    Payload::MqttPublishNotification(p),
                ))
            }),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
        );

        let mqtt_receiver = mqtt::run_receiver(
            mqtt_connection,
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new(
                    "MQTT",
                    Payload::MqttClientNotification(p),
                ))
            }),
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("MQTT", Payload::ValveCommand(p)))
            }),
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("MQTT", Payload::WaterMeterCommand(p)))
            }),
        );

        self.bind(async move {
            try_join(mqtt_sender, mqtt_receiver).await?;

            Ok(())
        })
    }

    pub fn button(
        mut self,
        id: ButtonId,
        source: &'static str,
        pin: impl InputPin<Error = impl Debug + 'static> + 'static,
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    > {
        let timer = self.timers.timer()?;
        let bc_sender = self.bc_sender.clone();

        self.bind(button::run(
            id,
            pin,
            timer,
            adapt::sender(bc_sender.clone(), move |p| {
                Some(BroadcastEvent::new(source, Payload::ButtonCommand(p)))
            }),
            PressedLevel::Low,
        ))
    }

    pub fn screen(
        mut self,
        display: impl FlushableDrawTarget<Color = impl RgbColor, Error = impl Debug> + Send + 'static,
    ) -> anyhow::Result<
        BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
    > {
        let (de_sender, de_receiver) = self.signal_factory.create()?;

        let screen = screen::run_screen(
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            self.valve_state.get(),
            self.water_meter_state.get(),
            self.battery_state.get(),
            de_sender,
        );

        let draw_engine = screen::run_draw_engine::<U, _, _>(de_receiver, display);

        self.bind(async move {
            try_join(screen, draw_engine).await?;

            Ok(())
        })
    }

    fn bind(
        self,
        fut: impl Future<Output = anyhow::Result<()>> + 'static,
    ) -> anyhow::Result<
        // TODO: Results in an extremely slow build BroadcastBinder<U, MV, MW, MB, S, R, T, N, impl Future<Output = anyhow::Result<()>>>,
        BroadcastBinder<
            U,
            MV,
            MW,
            MB,
            S,
            R,
            T,
            N,
            Pin<Box<dyn Future<Output = anyhow::Result<()>>>>,
        >,
    > {
        let joined_fut = self.joined_fut;

        Ok(BroadcastBinder {
            _unblocker: PhantomData,
            bc_sender: self.bc_sender,
            bc_receiver: self.bc_receiver,
            timers: self.timers,
            signal_factory: self.signal_factory,
            valve_state: self.valve_state,
            water_meter_state: self.water_meter_state,
            battery_state: self.battery_state,
            joined_fut: Box::pin(try_join(joined_fut, fut).map(|_| Ok(()))),
        })
    }

    pub fn into_future(self) -> impl Future<Output = anyhow::Result<()>> {
        self.joined_fut
    }
}
