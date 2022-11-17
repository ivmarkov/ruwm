use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Arc, Circle, Line, Primitive, PrimitiveStyle};
use embedded_graphics::Drawable;

use super::util::clear_cropped;
use super::Color;

#[derive(Clone, Debug)]
pub struct Wifi {
    pub padding: u32,
    pub outline: u32,
    pub strength: Option<u8>,
}

impl Default for Wifi {
    fn default() -> Self {
        Self::new()
    }
}

impl Wifi {
    pub const fn new() -> Self {
        Self {
            padding: 10,
            outline: 4,
            strength: Some(100),
        }
    }

    pub fn draw<T>(&self, target: &mut T) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color>,
    {
        self.draw_shape(&mut clear_cropped(target, self.padding)?)
    }

    fn draw_shape<T>(&self, target: &mut T) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color> + OriginDimensions,
    {
        let bbox = target.bounding_box();
        let Size { width, height } = bbox.size;

        let center = Point::new(
            bbox.top_left.x + width as i32 / 2,
            bbox.top_left.y + height as i32 - self.outline as i32,
        );
        let diameter = height * 2 * 4 / 5;

        self.draw_arc(
            center,
            diameter,
            self.strength.map(|strength| strength > 80).unwrap_or(false),
            target,
        )?;

        self.draw_arc(
            center,
            diameter * 2 / 3,
            self.strength.map(|strength| strength > 40).unwrap_or(false),
            target,
        )?;

        self.draw_arc(
            center,
            diameter / 3,
            self.strength.map(|strength| strength > 20).unwrap_or(false),
            target,
        )?;

        Circle::with_center(center, self.outline * 2)
            .into_styled(PrimitiveStyle::with_fill(
                if self.strength.map(|strength| strength > 10).unwrap_or(false) {
                    Color::White
                } else {
                    Color::Gray
                },
            ))
            .draw(target)?;

        if self.strength.is_none() {
            Line::new(bbox.top_left, bbox.top_left + bbox.size - Point::new(1, 1))
                .into_styled(PrimitiveStyle::with_stroke(Color::White, self.outline))
                .draw(target)?;
        }

        Ok(())
    }

    fn draw_arc<T>(
        &self,
        center: Point,
        diameter: u32,
        strong_signal: bool,
        target: &mut T,
    ) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color>,
    {
        let color = if strong_signal {
            Color::White
        } else {
            Color::Gray
        };

        Arc::with_center(center, diameter, 45.0.deg(), 90.0.deg())
            .into_styled(PrimitiveStyle::with_stroke(color, self.outline))
            .draw(target)
    }
}
