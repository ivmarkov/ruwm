use core::fmt::Debug;

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};

use crate::channel::{Receiver, Sender};
use crate::notification::Notification;

const TIMEOUT: Duration = Duration::from_secs(20);
const REMAINING_TIME_TRIGGER: Duration = Duration::from_secs(1);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RemainingTime {
    Indefinite,
    Duration(Duration),
}

pub struct Keepalive {
    event_notif: Notification,
}

impl Keepalive {
    pub const fn new() -> Self {
        Self {
            event_notif: Notification::new(),
        }
    }

    pub fn event_sink(&self) -> &Notification {
        &self.event_notif
    }

    pub async fn process(
        &'static self,
        remaining_time_sink: impl Sender<Data = RemainingTime>,
        quit_sink: impl Sender<Data = ()>,
    ) {
        process(&self.event_notif, remaining_time_sink, quit_sink).await
    }
}

pub async fn process(
    mut event_source: impl Receiver<Data = ()>,
    mut remaining_time_sink: impl Sender<Data = RemainingTime>,
    mut quit_sink: impl Sender<Data = ()>,
) {
    let mut quit_time = Some(Instant::now() + TIMEOUT);
    let mut quit_time_sent = None;

    loop {
        let event = event_source.recv();
        let tick = Timer::after(Duration::from_secs(2) /*Duration::from_millis(500)*/);

        //pin_mut!(event, tick);

        let result = select(event, tick).await;
        let now = Instant::now();

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
