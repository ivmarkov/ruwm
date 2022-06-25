use embedded_graphics::{draw_target::DrawTarget, prelude::RgbColor};

use crate::{
    battery::BatteryState,
    screen::shapes::{self, BatteryChargedText},
};

pub struct Battery;

impl Battery {
    pub fn draw<D>(target: &mut D, state: Option<&BatteryState>) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
    {
        if let Some(state) = state {
            let percentage = state.voltage.map(|voltage| {
                (voltage * 100 / (BatteryState::MAX_VOLTAGE + BatteryState::LOW_VOLTAGE)) as u8
            });

            shapes::Battery::new(percentage, BatteryChargedText::Xor, true).draw(target)?;
        }

        Ok(())
    }
}
