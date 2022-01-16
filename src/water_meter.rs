use core::cell::RefCell;
use core::mem;
use core::time::Duration;

extern crate alloc;
use alloc::rc::Rc;

use embedded_svc::event_bus;
use embedded_svc::event_bus::Timer;
use embedded_svc::sys_time;

use crate::pulse_counter::*;
use crate::storage::*;

const FLOW_STATS_INSTANCES: usize = 10;

const DURATIONS: [Duration; FLOW_STATS_INSTANCES] = [
    Duration::from_secs(60),
    Duration::from_secs(60 * 2),
    Duration::from_secs(60 * 5),
    Duration::from_secs(60 * 30),
    Duration::from_secs(60 * 60),
    Duration::from_secs(60 * 60 * 6),
    Duration::from_secs(60 * 60 * 12),
    Duration::from_secs(60 * 60 * 24),
    Duration::from_secs(60 * 60 * 24 * 7),
    Duration::from_secs(60 * 60 * 24 * 30),
];

#[derive(Clone)]
pub struct FlowSnapshot {
    time: Duration,
    edges_count: u32,
}

impl FlowSnapshot {
    pub const fn new(current_time: Duration, current_edges_count: u32) -> Self {
        Self {
            time: current_time,
            edges_count: current_edges_count,
        }
    }

    pub fn is_measurement_due(
        &self,
        measurement_duration: Duration,
        current_time: Duration,
    ) -> bool {
        Self::is_aligned_measurement_due(self.time, current_time, measurement_duration)
    }

    pub fn flow_detected(&self, current_edges_count: u32) -> bool {
        self.statistics(current_edges_count) > 1
    }

    pub fn statistics(&self, current_edges_count: u32) -> u32 {
        current_edges_count - self.edges_count
    }

    fn is_nonaligned_measurement_due(
        start_time: Duration,
        current_time: Duration,
        measurement_duration: Duration,
    ) -> bool {
        current_time - start_time >= measurement_duration
    }

    fn is_aligned_measurement_due(
        start_time: Duration,
        current_time: Duration,
        measurement_duration: Duration,
    ) -> bool {
        let start_time = Duration::from_secs(
            start_time.as_secs() / measurement_duration.as_secs() * measurement_duration.as_secs(),
        );

        Self::is_nonaligned_measurement_due(start_time, current_time, measurement_duration)
    }
}

#[derive(Clone)]
pub struct FlowMeasurement {
    start: FlowSnapshot,
    end: FlowSnapshot,
}

impl FlowMeasurement {
    pub fn new(start: FlowSnapshot, end: FlowSnapshot) -> Self {
        Self { start, end }
    }
}

pub struct WaterMeterStats {
    installation: FlowSnapshot,

    watch_start: Option<FlowSnapshot>,
    most_recent: FlowSnapshot,

    snapshots: [FlowSnapshot; FLOW_STATS_INSTANCES],
    measurements: [Option<FlowMeasurement>; FLOW_STATS_INSTANCES],
}

impl WaterMeterStats {
    fn update(&mut self, pulse_counter: &mut PulseCounter, now: Duration) -> anyhow::Result<bool> {
        let ps_data = pulse_counter.swap_data(&super::pulse_counter::Data {
            wakeup_edges: self.watch_start.as_ref().map(|_| 2).unwrap_or(0),
            ..Default::default()
        })?;

        self.most_recent = FlowSnapshot::new(
            now,
            self.most_recent.edges_count + ps_data.edges_count as u32,
        );

        let mut updated = false;

        if self.most_recent.edges_count > self.most_recent.edges_count {
            if let Some(watch_start) = self.watch_start.as_ref() {
                if watch_start.flow_detected(self.most_recent.edges_count) {
                    updated = true;
                }
            }
        }

        for (index, snapshot) in self.snapshots.iter_mut().enumerate() {
            if snapshot.is_measurement_due(DURATIONS[index], now) {
                let prev = mem::replace(snapshot, self.most_recent.clone());
                self.measurements[index] =
                    Some(FlowMeasurement::new(prev, self.most_recent.clone()));

                updated = true;
            }
        }

        Ok(updated)
    }
}

pub struct WaterMeter<S, E, T, N> {
    event_bus: E,
    sys_time: N,
    pulse_counter: PulseCounter,
    storage: S,
    stats: WaterMeterStats,
    timer: T,
}

impl<'a, S, E, N> WaterMeter<S, E, E::Timer<'a>, N>
where
    S: Storage<WaterMeterStats> + 'a,
    E: event_bus::EventBus<'a>,
    N: sys_time::SystemTime + 'a,
{
    pub const EVENT_SOURCE: event_bus::Source<()> = event_bus::Source::new("WATER_METER");

    pub fn new(
        event_bus: E,
        sys_time: N,
        pulse_counter: PulseCounter,
        storage: S,
    ) -> Result<Rc<RefCell<Self>>, anyhow::Error> {
        let state = Self {
            timer: event_bus
                .timer(event_bus::Priority::VeryHigh, Self::EVENT_SOURCE.id())
                .map_err(|e| anyhow::anyhow!(e))?,
            event_bus,
            sys_time,
            pulse_counter,
            stats: storage.get(),
            storage,
        };

        let state = Rc::new(RefCell::new(state));
        let weak = Rc::downgrade(&state);

        {
            let timer = &mut state.borrow_mut().timer;

            timer
                .callback(Some(move || {
                    weak.upgrade()
                        .map(|state| state.borrow_mut().poll())
                        .unwrap_or(Ok(()))
                }))
                .map_err(|e| anyhow::anyhow!(e))?;

            timer
                .schedule(Duration::from_secs(0))
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(state)
    }

    fn poll(&mut self) -> anyhow::Result<()> {
        if self
            .stats
            .update(&mut self.pulse_counter, self.sys_time.now())
            .unwrap()
        {
            self.event_bus
                .post(event_bus::Priority::VeryHigh, &Self::EVENT_SOURCE, &());
        }

        self.timer.schedule(Duration::from_millis(500));

        Ok(())
    }
}
