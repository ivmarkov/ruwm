use embedded_graphics::draw_target::DrawTarget;

use crate::battery::BatteryState;
use crate::screen::shapes::{self, Color};

pub struct Battery;

impl Battery {
    pub fn draw<D>(target: &mut D, state: Option<&BatteryState>) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        if let Some(state) = state {
            shapes::Battery {
                charged_percentage: state.percentage(),
                ..Default::default()
            }
            .draw(target)?;
        }

        Ok(())
    }
}
