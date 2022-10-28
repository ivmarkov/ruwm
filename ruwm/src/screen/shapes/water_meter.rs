use core::str;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::mono_font::*;
use embedded_graphics::prelude::{OriginDimensions, Point, Primitive, Size};
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};
use embedded_graphics::Drawable;

use super::util::{clear, draw, fill, text, to_str};
use super::Color;

pub struct WaterMeterClassic<const DIGITS: usize = 8> {
    edges_count: Option<u64>,
    divider: u32,
    outline: bool,
}

impl<const DIGITS: usize> WaterMeterClassic<DIGITS> {
    pub const SIZE: Size = Size::new(Self::WIDTH, Self::HEIGHT);
    pub const WIDTH: u32 = Self::FONT.character_size.width * DIGITS as u32 + Self::PADDING * 2;
    pub const HEIGHT: u32 = Self::FONT.character_size.height + Self::PADDING * 2;

    const FONT: MonoFont<'static> = profont::PROFONT_24_POINT;

    const PADDING: u32 = 2;
    const PADDED_WIDTH: u32 = Self::WIDTH - Self::PADDING * 2;
    const PADDED_HEIGHT: u32 = Self::HEIGHT - Self::PADDING * 2;

    pub fn new(edges_count: Option<u64>, divider: u32, outline: bool) -> Self {
        Self {
            edges_count,
            divider,
            outline,
        }
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        // Clear the area
        Rectangle::new(Point::new(0, 0), Self::SIZE)
            .into_styled(PrimitiveStyle::with_fill(Color::Black))
            .draw(target)?;

        self.draw_shape(&mut target.cropped(&Rectangle::new(
            Point::new(Self::PADDING as _, Self::PADDING as _),
            Size::new(Self::PADDED_WIDTH, Self::PADDED_HEIGHT),
        )))
    }

    fn draw_shape<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color> + OriginDimensions,
    {
        let bbox = target.bounding_box();

        if self.outline && DIGITS > 5 {
            bbox.into_styled(PrimitiveStyle::with_stroke(Color::Red, 2))
                .draw(target)?;

            Rectangle::new(
                Point::new(
                    bbox.top_left.x + Self::FONT.character_size.width as i32 * 5,
                    bbox.top_left.y,
                ),
                Size::new(
                    Self::FONT.character_size.width * (DIGITS as u32 - 5),
                    bbox.size.height,
                ),
            )
            .into_styled(PrimitiveStyle::with_fill(Color::Red))
            .draw(target)?;
        }

        let wm_text = if let Some(edges_count) = self.edges_count {
            let mut wm_text = [b'0'; DIGITS];
            to_str(edges_count / self.divider as u64, &mut wm_text);

            wm_text
        } else {
            [b'?'; DIGITS]
        };

        text(
            &Self::FONT,
            target,
            Point::zero(),
            str::from_utf8(&wm_text).unwrap(),
            Color::White,
            None,
        )?;

        Ok(())
    }
}

pub struct WaterMeterFract<const DIGITS: usize> {
    edges_count: Option<u64>,
    divider: u32,
}

impl<const DIGITS: usize> WaterMeterFract<DIGITS> {
    pub const SIZE: Size = Size::new(Self::WIDTH, Self::HEIGHT);
    pub const WIDTH: u32 = Self::FONT.character_size.width * DIGITS as u32 + Self::PADDING * 2;
    pub const HEIGHT: u32 = Self::FONT.character_size.height + Self::PADDING * 2;

    const FONT: MonoFont<'static> = profont::PROFONT_24_POINT;

    const PADDING: u32 = 2;
    const PADDED_WIDTH: u32 = Self::WIDTH - Self::PADDING * 2;
    const PADDED_HEIGHT: u32 = Self::HEIGHT - Self::PADDING * 2;

    pub fn new(edges_count: Option<u64>, divider: u32) -> Self {
        Self {
            edges_count,
            divider,
        }
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        // Clear the area
        clear(&Rectangle::new(Point::new(0, 0), Self::SIZE), target)?;

        self.draw_shape(&mut target.cropped(&Rectangle::new(
            Point::new(Self::PADDING as _, Self::PADDING as _),
            Size::new(Self::PADDED_WIDTH, Self::PADDED_HEIGHT),
        )))
    }

    fn draw_shape<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color> + OriginDimensions,
    {
        let bbox = target.bounding_box();

        if DIGITS > 5 {
            draw(&bbox, Color::Red, 2, target)?;

            fill(
                &Rectangle::new(
                    Point::new(
                        bbox.top_left.x + Self::FONT.character_size.width as i32 * 5,
                        bbox.top_left.y,
                    ),
                    Size::new(
                        Self::FONT.character_size.width * (DIGITS as u32 - 5),
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
            &Self::FONT,
            target,
            Point::zero(),
            str::from_utf8(&wm_text).unwrap(),
            Color::White,
            None,
        )?;

        Ok(())
    }

    fn draw_text<D>(
        &self,
        target: &mut D,
        position: Point,
        text: &str,
        color: Color,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        let character_style = MonoTextStyleBuilder::new()
            .font(&Self::FONT)
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
