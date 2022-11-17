use embedded_graphics::draw_target::DrawTarget;

use crate::battery::BatteryState;
use crate::screen::shapes::{self, Color};

use super::with_title;

pub struct Battery;

impl Battery {
    pub fn draw<T>(target: &mut T, page_changed: bool, state: Option<&BatteryState>) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color>,
    {
        let mut target = with_title(target, page_changed, "Battery")?;

        if let Some(state) = state {
            shapes::Battery {
                charged_percentage: state.percentage(),
                ..Default::default()
            }
            .draw(&mut target)?;
        }

        Ok(())
    }
}
