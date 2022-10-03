use core::fmt::Debug;

use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Timer};

use crate::notification::Notification;
use crate::quit;
use crate::state::State;
use crate::web;

const TIMEOUT: Duration = Duration::from_secs(20);
const REMAINING_TIME_TRIGGER: Duration = Duration::from_secs(1);

pub static STATE: State<RemainingTime, 1, { web::NOTIFY_SIZE }> = State::new(
    "REMAINING TIME",
    RemainingTime::Duration(TIMEOUT),
    [&crate::screen::REMAINING_TIME_NOTIF],
    web::NOTIFY.remaining_time.as_ref(),
);

pub(crate) static NOTIF: Notification = Notification::new();

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RemainingTime {
    Indefinite,
    Duration(Duration),
}

pub async fn process() {
    let mut quit_time = Some(Instant::now() + TIMEOUT);
    let mut quit_time_sent = None;

    loop {
        let result = select(
            NOTIF.wait(),
            Timer::after(Duration::from_secs(2) /*Duration::from_millis(500)*/),
        )
        .await;

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
            quit::QUIT.notify();
        } else if quit_time.is_some() != quit_time_sent.is_some()
            || quit_time_sent
                .map(|quit_time_sent| quit_time_sent + REMAINING_TIME_TRIGGER <= now)
                .unwrap_or(false)
        {
            quit_time_sent = Some(now);

            let remaining_time = quit_time
                .map(|quit_time| RemainingTime::Duration(quit_time - now))
                .unwrap_or(RemainingTime::Indefinite);

            STATE.update(remaining_time);
        }
    }
}
