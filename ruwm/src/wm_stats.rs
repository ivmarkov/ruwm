use embassy_time::{Duration, Instant, Timer};

use embassy_futures::select::{select, Either};

use crate::channel::Receiver;
use crate::notification::Notification;
use crate::{state::*, wm};

pub use crate::dto::water_meter_stats::*;

pub static STATE_NOTIFY: &[&Notification] = &[
    &crate::keepalive::NOTIF,
    &crate::screen::WM_STATS_STATE_NOTIF,
    &STATE_PERSIST_NOTIFY,
];

pub static STATE: State<WaterMeterStatsState> = State::new(WaterMeterStatsState::new());

pub static WM_STATE_NOTIF: Notification = Notification::new();

static STATE_PERSIST_NOTIFY: Notification = Notification::new();

pub async fn process() {
    let mut wm_state_source = (&WM_STATE_NOTIF, &wm::STATE);

    loop {
        let wm_state = wm_state_source.recv();
        let tick = Timer::after(Duration::from_secs(10) /*Duration::from_millis(200)*/);

        let edges_count = match select(wm_state, tick).await {
            Either::First(wm_state) => wm_state.edges_count,
            Either::Second(_) => STATE.get().most_recent.edges_count,
        };

        STATE
            .update_with(
                "WM STATS",
                |mut state| {
                    state.update(edges_count, Instant::now().as_secs());

                    state
                },
                STATE_NOTIFY,
            )
            .await;
    }
}

pub async fn persist(mut persister: impl FnMut(WaterMeterStatsState)) {
    loop {
        STATE_PERSIST_NOTIFY.wait().await;

        persister(STATE.get());
    }
}
