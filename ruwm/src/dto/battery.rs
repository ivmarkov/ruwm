use core::{
    cmp::{max, min},
    fmt::Debug,
};

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BatteryState {
    pub voltage: Option<u16>,
    pub powered: Option<bool>,
}

impl BatteryState {
    pub const LOW_VOLTAGE: u16 = 2700;
    pub const MAX_VOLTAGE: u16 = 3100;

    pub const fn new() -> Self {
        Self {
            voltage: None,
            powered: None,
        }
    }

    pub fn percentage(&self) -> Option<u8> {
        self.voltage.map(|voltage| {
            let voltage = max(
                BatteryState::LOW_VOLTAGE,
                min(BatteryState::MAX_VOLTAGE, voltage),
            );

            ((voltage - BatteryState::LOW_VOLTAGE) * 100
                / (BatteryState::MAX_VOLTAGE - BatteryState::LOW_VOLTAGE)) as u8
        })
    }
}
