use core::fmt::Debug;

use serde::{Deserialize, Serialize};

const FLOW_STATS_INSTANCES: usize = 8;

const DURATIONS: [u64; FLOW_STATS_INSTANCES] = [
    60 * 5,
    60 * 30,
    60 * 60,
    60 * 60 * 6,
    60 * 60 * 12,
    60 * 60 * 24,
    60 * 60 * 24 * 7,
    60 * 60 * 24 * 30,
];

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FlowSnapshot {
    pub time_secs: u64,
    pub edges_count: u64,
}

impl FlowSnapshot {
    pub const fn new(current_time_secs: u64, current_edges_count: u64) -> Self {
        Self {
            time_secs: current_time_secs,
            edges_count: current_edges_count,
        }
    }

    /// Get a reference to the flow snapshot's time.
    pub fn time_secs(&self) -> u64 {
        self.time_secs
    }

    /// Get a reference to the flow snapshot's edges count.
    pub fn edges_count(&self) -> u64 {
        self.edges_count
    }

    pub fn is_measurement_due(
        &self,
        measurement_duration_secs: u64,
        current_time_secs: u64,
    ) -> bool {
        Self::is_aligned_measurement_due(
            self.time_secs,
            current_time_secs,
            measurement_duration_secs,
        )
    }

    pub fn flow_detected(&self, current_edges_count: u64) -> bool {
        self.statistics(current_edges_count) > 1
    }

    pub fn statistics(&self, current_edges_count: u64) -> u64 {
        current_edges_count - self.edges_count
    }

    fn is_nonaligned_measurement_due(
        start_time_secs: u64,
        current_time_secs: u64,
        measurement_duration_secs: u64,
    ) -> bool {
        current_time_secs - start_time_secs >= measurement_duration_secs
    }

    fn is_aligned_measurement_due(
        start_time_secs: u64,
        current_time_secs: u64,
        measurement_duration_secs: u64,
    ) -> bool {
        let start_time_secs =
            start_time_secs / measurement_duration_secs * measurement_duration_secs;

        Self::is_nonaligned_measurement_due(
            start_time_secs,
            current_time_secs,
            measurement_duration_secs,
        )
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FlowMeasurement {
    start: FlowSnapshot,
    end: FlowSnapshot,
}

impl FlowMeasurement {
    pub const fn new(start: FlowSnapshot, end: FlowSnapshot) -> Self {
        Self { start, end }
    }

    pub fn start(&self) -> &FlowSnapshot {
        &self.start
    }

    pub fn end(&self) -> &FlowSnapshot {
        &self.end
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WaterMeterStatsState {
    pub installation: FlowSnapshot,

    pub most_recent: FlowSnapshot,

    pub snapshots: [FlowSnapshot; FLOW_STATS_INSTANCES],
    pub measurements: [Option<FlowMeasurement>; FLOW_STATS_INSTANCES],
}

impl WaterMeterStatsState {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update(&mut self, edges_count: u64, now_secs: u64) -> bool {
        let most_recent = FlowSnapshot::new(now_secs, edges_count);

        let mut updated = self.most_recent != most_recent;
        if updated {
            self.most_recent = most_recent;
        }

        for (index, snapshot) in self.snapshots.iter_mut().enumerate() {
            if snapshot.is_measurement_due(DURATIONS[index], now_secs) {
                let prev = core::mem::replace(snapshot, self.most_recent);
                self.measurements[index] = Some(FlowMeasurement::new(prev, self.most_recent));

                updated = true;
            }
        }

        updated
    }
}
