use core::fmt::Debug;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WaterMeterState {
    pub edges_count: u64,
    pub armed: bool,
    pub leaking: bool,
}

impl WaterMeterState {
    pub const fn new() -> Self {
        Self {
            edges_count: 0,
            armed: false,
            leaking: false,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum WaterMeterCommand {
    Arm,
    Disarm,
}
