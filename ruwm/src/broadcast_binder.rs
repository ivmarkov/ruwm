use core::fmt::Display;
use core::future::Future;
use core::pin::Pin;
use core::{fmt::Debug, marker::PhantomData};

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec::Vec;

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
use futures::try_join;

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

pub trait Notif {
    type Sender<D>: Sender<Data = D>;
    type Receiver<D>: Receiver<Data = D>;

    fn create<D>(&mut self) -> anyhow::Result<(Self::Sender<D>, Self::Receiver<D>)>
    where
        D: Send + Sync + Clone + 'static;
}

pub struct BroadcastBinder<U, MV, MW, MB, S, R, T, N> {
    _unblocker: PhantomData<U>,
    bc_sender: S,
    bc_receiver: R,
    timers: T,
    notif: N,
    valve_state: StateSnapshot<MV>,
    water_meter_state: StateSnapshot<MW>,
    battery_state: StateSnapshot<MB>,
    bindings: Vec<Pin<Box<dyn Future<Output = anyhow::Result<()>>>>>,
}

impl<U, MV, MW, MB, S, R, T, N> BroadcastBinder<U, MV, MW, MB, S, R, T, N>
where
    U: Unblocker + 'static,
    MV: Mutex<Data = Option<ValveState>> + Send + Sync + 'static,
    MW: Mutex<Data = WaterMeterState> + Send + Sync + 'static,
    MB: Mutex<Data = BatteryState> + Send + Sync + 'static,
    S: Sender<Data = BroadcastEvent> + Clone + 'static,
    R: Receiver<Data = BroadcastEvent> + Clone + 'static,
    T: TimerService + 'static,
    N: Notif + 'static,
{
    pub fn new(bc_sender: S, bc_receiver: R, timers: T, notif: N) -> Self {
        Self {
            _unblocker: PhantomData,
            bc_sender,
            bc_receiver,
            timers,
            notif,
            valve_state: StateSnapshot::<MV>::new(),
            water_meter_state: StateSnapshot::<MW>::new(),
            battery_state: StateSnapshot::<MB>::new(),
            bindings: Vec::new(),
        }
    }

    pub fn event_logger(&mut self) -> anyhow::Result<&mut Self> {
        self.bind(event_logger::run(self.bc_receiver.clone()))
    }

    pub fn emergency(&mut self) -> anyhow::Result<&mut Self> {
        self.bind(emergency::run(
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("EMERGENCY", Payload::ValveCommand(p)))
            }),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
        ))
    }

    pub fn wifi(&mut self, wifi: impl Receiver + 'static) -> anyhow::Result<&mut Self> {
        self.bind(pipe::run(
            wifi,
            adapt::sender(self.bc_sender.clone(), |_| {
                Some(BroadcastEvent::new("WIFI", Payload::WifiStatus))
            }),
        ))
    }

    pub fn valve<P, PO, PC>(
        &mut self,
        power_pin: P,
        open_pin: PO,
        close_pin: PC,
    ) -> anyhow::Result<&mut Self>
    where
        P: OutputPin + 'static,
        P::Error: Debug,
        PO: OutputPin + 'static,
        PO::Error: Debug,
        PC: OutputPin + 'static,
        PC::Error: Debug,
    {
        let (vsc_sender, vsc_receiver) = self.notif.create()?;
        let (vsn_sender, vsn_receiver) = self.notif.create()?;

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
            try_join! {
                valve_events,
                valve_spin,
            }?;

            Ok(())
        })
    }

    pub fn water_meter(
        &mut self,
        pulse_counter: impl PulseCounter + 'static,
    ) -> anyhow::Result<&mut Self> {
        let timer = self.timers.timer()?;

        self.bind(water_meter::run(
            self.water_meter_state.clone(),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("WM", Payload::WaterMeterState(p)))
            }),
            timer,
            pulse_counter,
        ))
    }

    pub fn battery<ADC: 'static, BP, PP>(
        &mut self,
        one_shot: impl adc::OneShot<ADC, u16, BP> + 'static,
        battery_pin: BP,
        power_pin: PP,
    ) -> anyhow::Result<&mut Self>
    where
        BP: adc::Channel<ADC> + 'static,
        PP: InputPin + 'static,
        PP::Error: Debug,
    {
        let timer = self.timers.timer()?;

        self.bind(battery::run(
            self.battery_state.clone(),
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("BATTERY", Payload::BatteryState(p)))
            }),
            timer,
            one_shot,
            battery_pin,
            power_pin,
        ))
    }

    pub fn mqtt<M>(
        &mut self,
        topic_prefix: impl AsRef<str> + 'static,
        mqtt_client: impl Client + Publish + 'static,
        mqtt_connection: M,
    ) -> anyhow::Result<&mut Self>
    where
        M: Connection + 'static,
        M::Error: Display,
    {
        let mqtt_sender = mqtt::run_sender(
            topic_prefix,
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
            try_join! {
                mqtt_sender,
                mqtt_receiver,
            }?;

            Ok(())
        })
    }

    pub fn button<P>(
        &mut self,
        id: ButtonId,
        source: &'static str,
        pin: P,
    ) -> anyhow::Result<&mut Self>
    where
        P: InputPin + 'static,
        P::Error: Debug,
    {
        let timer = self.timers.timer()?;

        self.bind(button::run(
            id,
            pin,
            timer,
            adapt::sender(self.bc_sender.clone(), move |p| {
                Some(BroadcastEvent::new(source, Payload::ButtonCommand(p)))
            }),
            PressedLevel::Low,
        ))
    }

    pub fn screen<D>(&mut self, display: D) -> anyhow::Result<&mut Self>
    where
        D: FlushableDrawTarget,
        D: FlushableDrawTarget + Send + 'static,
        D::Color: RgbColor,
        D::Error: Debug,
    {
        let (de_sender, de_receiver) = self.notif.create()?;

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

        let draw_engine = screen::run_draw_engine::<U, _>(de_receiver, display);

        self.bind(async move {
            try_join! {
                screen,
                draw_engine,
            }?;

            Ok(())
        })
    }

    pub fn run(self) -> impl Future<Output = anyhow::Result<()>> {
        let mut fut: Pin<Box<dyn Future<Output = anyhow::Result<()>>>> =
            Box::pin(futures::future::ready(Ok(())));

        for b in self.bindings {
            fut = Box::pin(async move {
                try_join! {
                    fut,
                    b,
                }?;

                Ok(())
            });
        }

        fut
    }

    fn bind(
        &mut self,
        fut: impl Future<Output = anyhow::Result<()>> + 'static,
    ) -> anyhow::Result<&mut Self> {
        self.bindings.push(Box::pin(fut));

        Ok(self)
    }
}
