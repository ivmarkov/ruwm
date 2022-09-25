use embassy_time::{Duration, Instant, Timer};

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};

use crate::channel::{Receiver, Sender};
use crate::notification::Notification;
use crate::state::*;
use crate::water_meter::WaterMeterState;

pub use crate::dto::water_meter_stats::*;

pub struct WaterMeterStats<R>
where
    R: RawMutex,
{
    state: CachingStateCell<
        R,
        MemoryStateCell<NoopRawMutex, Option<WaterMeterStatsState>>,
        MutRefStateCell<NoopRawMutex, WaterMeterStatsState>,
    >,
    wm_state_notif: Notification,
}

impl<R> WaterMeterStats<R>
where
    R: RawMutex + Send + Sync + 'static,
{
    pub fn new(state: &'static mut WaterMeterStatsState) -> Self {
        Self {
            state: CachingStateCell::new(MemoryStateCell::new(None), MutRefStateCell::new(state)),
            wm_state_notif: Notification::new(),
        }
    }

    pub fn state(&self) -> &(impl StateCellRead<Data = WaterMeterStatsState> + Send + Sync) {
        &self.state
    }

    pub fn wm_state_sink(&self) -> &Notification {
        &self.wm_state_notif
    }

    pub async fn process(
        &'static self,
        wm_state: &'static (impl StateCellRead<Data = WaterMeterState> + Send + Sync + 'static),
        state_sink: impl Sender<Data = ()>,
    ) {
        process(&self.state, (&self.wm_state_notif, wm_state), state_sink).await
    }
}

pub async fn process(
    state: &impl StateCell<Data = WaterMeterStatsState>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut state_sink: impl Sender<Data = ()>,
) {
    loop {
        let wm_state = wm_state_source.recv();
        let tick = Timer::after(Duration::from_secs(10) /*Duration::from_millis(200)*/);

        //pin_mut!(wm_state, tick);

        let edges_count = match select(wm_state, tick).await {
            Either::First(wm_state) => wm_state.edges_count,
            Either::Second(_) => state.get().most_recent.edges_count,
        };

        update_with(
            "WM STATS",
            state,
            |mut state| {
                state.update(edges_count, Instant::now().as_secs());

                state
            },
            &mut state_sink,
        )
        .await;
    }
}
