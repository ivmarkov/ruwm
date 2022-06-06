use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::signal::asynch::{SendSyncSignalFamily, Signal};
use embedded_svc::sys_time::SystemTime;
use embedded_svc::timer::asynch::OnceTimer;
use embedded_svc::utils::asynch::channel::adapt::adapt;
use embedded_svc::utils::asynch::select::{select, Either};
use embedded_svc::utils::asynch::signal::adapt::as_channel;

use crate::utils::as_static_receiver;

const TIMEOUT: Duration = Duration::from_secs(20);
const REMAINING_TIME_TRIGGER: Duration = Duration::from_secs(1);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemainingTime {
    Indefinite,
    Duration(Duration),
}

pub struct Keepalive<M>
where
    M: SendSyncSignalFamily,
{
    event_signal: M::Signal<()>,
}

impl<M> Keepalive<M>
where
    M: SendSyncSignalFamily,
{
    pub fn new() -> Self {
        Self {
            event_signal: M::Signal::new(),
        }
    }

    pub fn event_sink<D>(&'static self) -> impl Sender<Data = D> + 'static
    where
        D: Send + 'static,
    {
        adapt(as_channel(&self.event_signal), |_| Some(()))
    }

    pub async fn process(
        &'static self,
        timer: impl OnceTimer,
        system_time: impl SystemTime,
        remaining_time_sink: impl Sender<Data = RemainingTime>,
        quit_sink: impl Sender<Data = ()>,
    ) {
        process(
            timer,
            system_time,
            as_static_receiver(&self.event_signal),
            remaining_time_sink,
            quit_sink,
        )
        .await
    }
}

pub async fn process(
    mut timer: impl OnceTimer,
    system_time: impl SystemTime,
    mut event_source: impl Receiver<Data = ()>,
    mut remaining_time_sink: impl Sender<Data = RemainingTime>,
    mut quit_sink: impl Sender<Data = ()>,
) {
    let mut quit_time = Some(system_time.now() + TIMEOUT);
    let mut quit_time_sent = None;

    loop {
        let event = event_source.recv();
        let tick = timer
            .after(Duration::from_secs(2) /*Duration::from_millis(500)*/)
            .unwrap();

        //pin_mut!(event, tick);

        let result = select(event, tick).await;
        let now = system_time.now();

        if let Either::First(_) = result {
            quit_time = Some(now + TIMEOUT);

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
        }

        if quit_time.map(|quit_time| now >= quit_time).unwrap_or(false) {
            quit_sink.send(()).await;
        } else if quit_time.is_some() != quit_time_sent.is_some()
            || quit_time_sent
                .map(|quit_time_sent| quit_time_sent + REMAINING_TIME_TRIGGER <= now)
                .unwrap_or(false)
        {
            quit_time_sent = Some(now);

            let remaining_time = quit_time
                .map(|quit_time| RemainingTime::Duration(quit_time - now))
                .unwrap_or(RemainingTime::Indefinite);

            remaining_time_sink.send(remaining_time).await;
        }
    }
}
