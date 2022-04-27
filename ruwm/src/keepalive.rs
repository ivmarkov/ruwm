use core::time::Duration;

use futures::future::{select, Either};
use futures::pin_mut;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::sys_time::SystemTime;
use embedded_svc::timer::asyncs::OnceTimer;

use crate::broadcast_event::{BroadcastEvent, Payload};
use crate::error;
use crate::quit::Quit;

const TIMEOUT: Duration = Duration::from_secs(20);
const REMAINING_TIME_TRIGGER: Duration = Duration::from_secs(1);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemainingTime {
    Indefinite,
    Duration(Duration),
}

pub async fn run(
    mut event: impl Receiver<Data = BroadcastEvent>,
    mut timer: impl OnceTimer,
    system_time: impl SystemTime,
    mut notif: impl Sender<Data = RemainingTime>,
    mut quit: impl Sender<Data = Quit>,
) -> error::Result<()> {
    let mut quit_time = Some(system_time.now() + TIMEOUT);
    let mut quit_time_sent = None;

    loop {
        let event = event.recv();
        let tick = timer
            .after(Duration::from_secs(2) /*Duration::from_millis(500)*/)
            .map_err(error::svc)?;

        pin_mut!(event, tick);

        let result = select(event, tick).await;
        let now = system_time.now();

        if let Either::Left((event, _)) = result {
            let event = event.map_err(error::svc)?;

            quit_time = match event.payload() {
                Payload::ValveCommand(_)
                | Payload::ValveState(_)
                | Payload::WaterMeterCommand(_)
                | Payload::WaterMeterState(_)
                | Payload::ButtonCommand(_)
                | Payload::MqttClientNotification(_)
                | Payload::WebResponse(_, _) => Some(now + TIMEOUT),
                Payload::BatteryState(battery_state) => {
                    battery_state.powered.unwrap_or(true).then(|| now + TIMEOUT)
                }
                _ => quit_time,
            };
        }

        if quit_time.map(|quit_time| now >= quit_time).unwrap_or(false) {
            quit.send(Quit).await?;
        } else if quit_time.is_some() != quit_time_sent.is_some()
            || quit_time_sent
                .map(|quit_time_sent| quit_time_sent + REMAINING_TIME_TRIGGER <= now)
                .unwrap_or(false)
        {
            quit_time_sent = Some(now);

            let remaining_time = quit_time
                .map(|quit_time| RemainingTime::Duration(quit_time - now))
                .unwrap_or(RemainingTime::Indefinite);

            notif.send(remaining_time).await?;
        }
    }
}
