use embassy_time::{Duration, Instant, Timer};

use embassy_futures::select::{select, Either};

use crate::notification::Notification;
use crate::{state::*, wm};

pub use crate::dto::water_meter_stats::*;

pub static STATE: State<WaterMeterStatsState, 3> = State::new(
    WaterMeterStatsState::new(),
    [
        &crate::keepalive::NOTIF,
        &crate::screen::WM_STATS_STATE_NOTIF,
        &STATE_PERSIST_NOTIFY,
    ],
);

pub static WM_STATE_NOTIF: Notification = Notification::new();

static STATE_PERSIST_NOTIFY: Notification = Notification::new();

pub async fn process() {
    loop {
        let edges_count = match select(
            WM_STATE_NOTIF.wait(),
            Timer::after(Duration::from_secs(10) /*Duration::from_millis(200)*/),
        )
        .await
        {
            Either::First(_) => wm::STATE.get().edges_count,
            Either::Second(_) => STATE.get().most_recent.edges_count,
        };

        STATE.update_with("WM STATS", |mut state| {
            state.update(edges_count, Instant::now().as_secs());

            state
        });
    }
}

pub async fn persist(mut persister: impl FnMut(WaterMeterStatsState)) {
    loop {
        STATE_PERSIST_NOTIFY.wait().await;

        persister(STATE.get());
    }
}
