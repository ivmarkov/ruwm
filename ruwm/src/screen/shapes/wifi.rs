use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Arc, Circle, Line, Primitive, PrimitiveStyle, Rectangle};
use embedded_graphics::Drawable;

use super::util::clear;
use super::Color;

#[derive(Clone, Debug)]
pub struct Wifi {
    pub size: Size,
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
            size: Size::new(100, 100),
            padding: 10,
            outline: 4,
            strength: Some(100),
        }
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        // Clear the area
        clear(&Rectangle::new(Point::new(0, 0), self.size), target)?;

        self.draw_shape(&mut target.cropped(&Rectangle::new(
            Point::new(self.padding as _, self.padding as _),
            self.padded_size(),
        )))
    }

    fn draw_shape<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color> + OriginDimensions,
    {
        let Size { width, height } = target.size();

        let center = Point::new(width as i32 / 2, height as i32);

        self.draw_arc(
            center,
            height,
            self.strength.map(|strength| strength > 80).unwrap_or(false),
            target,
        )?;
        self.draw_arc(
            center,
            height * 2 / 3,
            self.strength.map(|strength| strength > 40).unwrap_or(false),
            target,
        )?;
        self.draw_arc(
            center,
            height / 3,
            self.strength.map(|strength| strength > 20).unwrap_or(false),
            target,
        )?;

        Circle::with_center(center, self.outline)
            .into_styled(PrimitiveStyle::with_fill(
                if self.strength.map(|strength| strength > 10).unwrap_or(false) {
                    Color::White
                } else {
                    Color::Gray
                },
            ))
            .draw(target)?;

        if self.strength.is_none() {
            Line::new(Point::zero(), Point::new(width as i32, height as i32))
                .into_styled(PrimitiveStyle::with_stroke(Color::White, self.outline))
                .draw(target)?;
        }

        Ok(())
    }

    fn draw_arc<D>(
        &self,
        center: Point,
        diameter: u32,
        strong_signal: bool,
        target: &mut D,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color> + OriginDimensions,
    {
        let color = if strong_signal {
            Color::White
        } else {
            Color::Gray
        };

        Arc::with_center(center, diameter, 270.0.deg(), 45.0.deg())
            .into_styled(PrimitiveStyle::with_stroke(color, self.outline))
            .draw(target)
    }

    fn padded_size(&self) -> Size {
        Size::new(
            self.size.width - self.padding * 2,
            self.size.height - self.padding * 2,
        )
    }
}
