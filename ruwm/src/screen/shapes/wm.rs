use core::str;

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::mono_font::*;
use embedded_graphics::prelude::{OriginDimensions, Point, Size};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};
use embedded_graphics::Drawable;

use super::util::{clear_cropped, draw, fill, text, to_str};
use super::Color;

pub struct WaterMeterClassic<'a, const DIGITS: usize = 8> {
    pub edges_count: Option<u64>,
    pub divider: u32,
    pub padding: u32,
    pub outline: u32,
    pub font: MonoFont<'a>,
}

impl<'a, const DIGITS: usize> WaterMeterClassic<'a, DIGITS> {
    pub const fn new() -> Self {
        Self {
            edges_count: None,
            divider: 1,
            padding: 2,
            outline: 2,
            font: profont::PROFONT_18_POINT,
        }
    }

    pub fn preferred_size(&self) -> Size {
        let width = self.font.character_size.width * DIGITS as u32 + self.padding * 2;
        let height = self.font.character_size.height + self.padding * 2;

        Size::new(width, height)
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

        if self.outline > 0 && DIGITS > 5 {
            draw(&bbox, Color::Red, self.outline, target)?;

            fill(
                &Rectangle::new(
                    Point::new(
                        bbox.top_left.x + self.font.character_size.width as i32 * 5,
                        bbox.top_left.y,
                    ),
                    Size::new(
                        self.font.character_size.width * (DIGITS as u32 - 5),
                        bbox.size.height,
                    ),
                ),
                Color::Red,
                target,
            )?;
        }

        let wm_text = if let Some(edges_count) = self.edges_count {
            let mut wm_text = [b'0'; DIGITS];
            to_str(edges_count / self.divider as u64, &mut wm_text);

            wm_text
        } else {
            [b'?'; DIGITS]
        };

        text(
            &self.font,
            target,
            bbox.top_left,
            str::from_utf8(&wm_text).unwrap(),
            Color::White,
            None,
        )?;

        Ok(())
    }
}

impl<'a, const DIGITS: usize> Default for WaterMeterClassic<'a, DIGITS> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WaterMeterFract<'a, const DIGITS: usize> {
    pub edges_count: Option<u64>,
    pub divider: u32,
    pub padding: u32,
    pub outline: u32,
    pub font: MonoFont<'a>,
}

impl<'a, const DIGITS: usize> WaterMeterFract<'a, DIGITS> {
    pub const fn new() -> Self {
        Self {
            edges_count: None,
            divider: 1,
            padding: 2,
            outline: 2,
            font: profont::PROFONT_18_POINT,
        }
    }

    pub fn preferred_size(&self) -> Size {
        let width = self.font.character_size.width * DIGITS as u32 + self.padding * 2;
        let height = self.font.character_size.height + self.padding * 2;

        Size::new(width, height)
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

        if DIGITS > 5 {
            draw(&bbox, Color::Red, 2, target)?;

            fill(
                &Rectangle::new(
                    Point::new(
                        bbox.top_left.x + self.font.character_size.width as i32 * 5,
                        bbox.top_left.y,
                    ),
                    Size::new(
                        self.font.character_size.width * (DIGITS as u32 - 5),
                        bbox.size.height,
                    ),
                ),
                Color::Red,
                target,
            )?;
        }

        let wm_text = if let Some(edges_count) = self.edges_count {
            let mut wm_text = [b'0'; DIGITS];
            to_str(edges_count / self.divider as u64, &mut wm_text);

            wm_text
        } else {
            [b'?'; DIGITS]
        };

        text(
            &self.font,
            target,
            bbox.top_left,
            str::from_utf8(&wm_text).unwrap(),
            Color::White,
            None,
        )?;

        Ok(())
    }

    fn draw_text<T>(
        &self,
        target: &mut T,
        position: Point,
        text: &str,
        color: Color,
    ) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color>,
    {
        let character_style = MonoTextStyleBuilder::new()
            .font(&self.font)
            .text_color(color)
            .build();

        let text_style = TextStyleBuilder::new()
            .baseline(Baseline::Top)
            .alignment(Alignment::Left)
            .build();

        Text::with_text_style(text, position, character_style, text_style).draw(target)?;

        Ok(())
    }
}
