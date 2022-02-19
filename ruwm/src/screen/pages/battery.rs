use embedded_graphics::{draw_target::DrawTarget, prelude::RgbColor};

use crate::{
    battery::BatteryState,
    screen::shapes::{self, BatteryChargedText},
};

pub struct Battery {
    state: Option<BatteryState>,
}

impl Battery {
    pub fn new() -> Self {
        Self { state: None }
    }

    pub fn draw<D>(&mut self, target: &mut D, state: BatteryState) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
    {
        if self.state != Some(state) {
            self.state = Some(state);

            let percentage = state.voltage.map(|voltage| {
                (voltage * 100 / (BatteryState::MAX_VOLTAGE + BatteryState::LOW_VOLTAGE)) as u8
            });

            shapes::Battery::new(percentage, BatteryChargedText::Xor, true).draw(target)?;
        }

        Ok(())
    }
}
