use embedded_graphics::{draw_target::DrawTarget, prelude::RgbColor, Drawable};

use crate::{battery::BatteryState, screen::shapes};

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
        if let Some(current_state) = self.state {
            if current_state == state {
                return Ok(());
            }
        }

        self.state = Some(state);

        let percentage = state.voltage.map(|voltage| {
            (voltage * 100 / (BatteryState::MAX_VOLTAGE + BatteryState::LOW_VOLTAGE)) as u8
        });

        shapes::battery::Battery::new(percentage, true, true).draw(target)?;

        Ok(())
    }
}
