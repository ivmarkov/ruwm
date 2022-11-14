use core::str;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::mono_font::*;
use embedded_graphics::prelude::{OriginDimensions, Point, Size};
use embedded_graphics::primitives::{Line, Primitive, PrimitiveStyle, Rectangle};
use embedded_graphics::Drawable;

use super::util::{clear, draw, text};
use super::Color;

pub struct Textbox<'a> {
    pub text: &'a str,
    pub color: Color,
    pub divider: u32,
    pub padding: u32,
    pub outline: u32,
    pub strikethrough: bool,
    pub font: MonoFont<'a>,
}

impl<'a> Textbox<'a> {
    pub const fn new() -> Self {
        Self {
            text: "???",
            color: Color::Yellow,
            divider: 1,
            padding: 2,
            outline: 2,
            strikethrough: false,
            font: profont::PROFONT_24_POINT,
        }
    }

    pub fn size(&self) -> Size {
        let width = self.font.character_size.width * self.text.len() as u32 + self.padding * 2;
        let height = self.font.character_size.height + self.padding * 2;

        Size::new(width, height)
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        let size = self.size();

        // Clear the area
        clear(&Rectangle::new(Point::new(0, 0), size), target)?;

        self.draw_shape(&mut target.cropped(&Rectangle::new(
            Point::new(self.padding as _, self.padding as _),
            Size::new(
                size.width - self.padding * 2,
                size.height - self.padding * 2,
            ),
        )))
    }

    fn draw_shape<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color> + OriginDimensions,
    {
        let bbox = target.bounding_box();

        if self.outline > 0 && self.strikethrough {
            draw(&bbox, self.color, self.outline, target)?;
        }

        text(
            &self.font,
            target,
            Point::zero(),
            self.text,
            self.color,
            None,
        )?;

        if self.strikethrough {
            Line::new(bbox.top_left, bbox.top_left + bbox.size - Point::new(1, 1))
                .into_styled(PrimitiveStyle::with_stroke(Color::White, self.outline))
                .draw(target)?;
        }

        Ok(())
    }
}

impl<'a> Default for Textbox<'a> {
    fn default() -> Self {
        Self::new()
    }
}
