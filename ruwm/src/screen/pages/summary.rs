use embedded_graphics::{draw_target::DrawTarget, prelude::RgbColor};

use crate::{
    battery::BatteryState,
    screen::shapes::{self, BatteryChargedText},
    valve::ValveState,
    water_meter::WaterMeterState,
};

pub struct Summary {
    valve_state: Option<Option<ValveState>>,
    water_meter_state: Option<WaterMeterState>,
    battery_state: Option<BatteryState>,
}

impl Summary {
    pub fn new() -> Self {
        Self {
            valve_state: None,
            water_meter_state: None,
            battery_state: None,
        }
    }

    pub fn draw<D>(
        &mut self,
        target: &mut D,
        valve_state: Option<ValveState>,
        water_meter_state: WaterMeterState,
        battery_state: BatteryState,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
    {
        if self.valve_state != Some(valve_state) {
            self.valve_state = Some(valve_state);

            // TODO
        }

        if self.water_meter_state != Some(water_meter_state) {
            self.water_meter_state = Some(water_meter_state);

            // TODO
        }

        if self.battery_state != Some(battery_state) {
            self.battery_state = Some(battery_state);

            let percentage = battery_state.voltage.map(|voltage| {
                (voltage as u32 * 100
                    / (BatteryState::MAX_VOLTAGE as u32 + BatteryState::LOW_VOLTAGE as u32))
                    as u8
            });

            // TODO shapes::Battery::new(percentage, BatteryChargedText::Xor, true).draw(target)?;
        }

        Ok(())
    }
}
