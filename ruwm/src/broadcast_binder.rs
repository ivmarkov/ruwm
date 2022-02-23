use core::fmt::Display;
use core::future::Future;
use core::{fmt::Debug, marker::PhantomData};

use embedded_svc::mqtt::client::nonblocking::{Client, Connection, Publish};
use futures::try_join;

use embedded_graphics::prelude::RgbColor;
use embedded_hal::adc;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use embedded_svc::mutex::Mutex;
use embedded_svc::unblocker::nonblocking::Unblocker;
use embedded_svc::{
    channel::nonblocking::{Receiver, Sender},
    timer::nonblocking::TimerService,
    utils::nonblocking::channel::adapt,
};

use crate::pulse_counter::PulseCounter;
use crate::state_snapshot::StateSnapshot;
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

pub struct BroadcastBinder<U, S, R, T, N> {
    _unblocker: PhantomData<U>,
    bc_sender: S,
    bc_receiver: R,
    timers: T,
    notif: N,
}

impl<U, S, R, T, N> BroadcastBinder<U, S, R, T, N>
where
    U: Unblocker,
    S: Sender<Data = BroadcastEvent> + Clone,
    R: Receiver<Data = BroadcastEvent> + Clone,
    T: TimerService,
    N: Notif,
{
    pub fn new(bc_sender: S, bc_receiver: R, timers: T, notif: N) -> Self {
        Self {
            _unblocker: PhantomData,
            bc_sender,
            bc_receiver,
            timers,
            notif,
        }
    }

    pub fn event_logger(&mut self) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>> {
        let event_logger = event_logger::run(self.bc_receiver.clone());

        Ok(event_logger)
    }

    pub fn emergency(&mut self) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>> {
        let emergency = emergency::run(
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("EMERGENCY", Payload::ValveCommand(p)))
            }),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::receiver(self.bc_receiver.clone(), Into::into),
        );

        Ok(emergency)
    }

    pub fn wifi(
        &mut self,
        wifi: impl Receiver,
    ) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>> {
        let wifi_notif = pipe::run(
            wifi,
            adapt::sender(self.bc_sender.clone(), |_| {
                Some(BroadcastEvent::new("WIFI", Payload::WifiStatus))
            }),
        );

        Ok(wifi_notif)
    }

    pub fn valve<P, PO, PC>(
        &mut self,
        power_pin: P,
        open_pin: PO,
        close_pin: PC,
        valve_state: StateSnapshot<impl Mutex<Data = Option<ValveState>> + Send + Sync>,
    ) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>>
    where
        P: OutputPin,
        P::Error: Debug,
        PO: OutputPin,
        PO::Error: Debug,
        PC: OutputPin,
        PC::Error: Debug,
    {
        let (vsc_sender, vsc_receiver) = self.notif.create()?;
        let (vsn_sender, vsn_receiver) = self.notif.create()?;

        let valve_events = valve::run_events(
            valve_state.clone(),
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

        let valve = async move {
            try_join! {
                valve_events,
                valve_spin,
            }?;

            Ok(())
        };

        Ok(valve)
    }

    pub fn water_meter(
        &mut self,
        water_meter_state: StateSnapshot<impl Mutex<Data = WaterMeterState> + Send + Sync>,
        pulse_counter: impl PulseCounter,
    ) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>> {
        let water_meter = water_meter::run(
            water_meter_state,
            adapt::receiver(self.bc_receiver.clone(), Into::into),
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("WM", Payload::WaterMeterState(p)))
            }),
            self.timers.timer()?,
            pulse_counter,
        );

        Ok(water_meter)
    }

    pub fn battery<ADC, BP, PP>(
        &mut self,
        battery_state: StateSnapshot<impl Mutex<Data = BatteryState> + Send + Sync>,
        one_shot: impl adc::OneShot<ADC, u16, BP>,
        battery_pin: BP,
        power_pin: PP,
    ) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>>
    where
        BP: adc::Channel<ADC>,
        PP: InputPin,
        PP::Error: Debug,
    {
        let battery = battery::run(
            battery_state,
            adapt::sender(self.bc_sender.clone(), |p| {
                Some(BroadcastEvent::new("BATTERY", Payload::BatteryState(p)))
            }),
            self.timers.timer()?,
            one_shot,
            battery_pin,
            power_pin,
        );

        Ok(battery)
    }

    pub fn mqtt<M>(
        &mut self,
        topic_prefix: impl AsRef<str>,
        mqtt_client: impl Client + Publish,
        mqtt_connection: M,
    ) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>>
    where
        M: Connection,
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

        let mqtt = async move {
            try_join! {
                mqtt_sender,
                mqtt_receiver,
            }?;

            Ok(())
        };

        Ok(mqtt)
    }

    pub fn button<P>(
        &mut self,
        id: ButtonId,
        source: &'static str,
        pin: P,
    ) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>>
    where
        P: InputPin,
        P::Error: Debug,
    {
        let button = button::run(
            id,
            pin,
            self.timers.timer()?,
            adapt::sender(self.bc_sender.clone(), move |p| {
                Some(BroadcastEvent::new(source, Payload::ButtonCommand(p)))
            }),
            PressedLevel::Low,
        );

        Ok(button)
    }

    pub fn screen<D>(
        &mut self,
        valve_state: Option<ValveState>,
        water_meter_state: WaterMeterState,
        battery_state: BatteryState,
        display: D,
    ) -> anyhow::Result<impl Future<Output = anyhow::Result<()>>>
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
            valve_state,
            water_meter_state,
            battery_state,
            de_sender,
        );

        let draw_engine = screen::run_draw_engine::<U, _>(de_receiver, display);

        let screen = async move {
            try_join! {
                screen,
                draw_engine,
            }?;

            Ok(())
        };

        Ok(screen)
    }
}
