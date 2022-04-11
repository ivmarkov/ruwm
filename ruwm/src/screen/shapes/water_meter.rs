use core::str;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::mono_font::*;
use embedded_graphics::prelude::{OriginDimensions, Point, Primitive, RgbColor, Size};
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};
use embedded_graphics::Drawable;

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
        D: DrawTarget,
        D::Color: RgbColor,
    {
        // Clear the area
        Rectangle::new(Point::new(0, 0), Self::SIZE)
            .into_styled(PrimitiveStyle::with_fill(D::Color::BLACK))
            .draw(target)?;

        self.draw_shape(&mut target.cropped(&Rectangle::new(
            Point::new(Self::PADDING as _, Self::PADDING as _),
            Size::new(Self::PADDED_WIDTH, Self::PADDED_HEIGHT),
        )))
    }

    fn draw_shape<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget + OriginDimensions,
        D::Color: RgbColor,
    {
        let bbox = target.bounding_box();

        if self.outline && DIGITS > 5 {
            bbox.into_styled(PrimitiveStyle::with_stroke(D::Color::RED, 2))
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
            .into_styled(PrimitiveStyle::with_fill(D::Color::RED))
            .draw(target)?;
        }

        let wm_text = if let Some(edges_count) = self.edges_count {
            let mut wm_text = [b'0'; DIGITS];
            to_str(edges_count / self.divider as u64, &mut wm_text);

            wm_text
        } else {
            [b'?'; DIGITS]
        };

        self.draw_text(
            target,
            Point::zero(),
            str::from_utf8(&wm_text).unwrap(),
            D::Color::WHITE,
        )?;

        Ok(())
    }

    fn draw_text<D>(
        &self,
        target: &mut D,
        position: Point,
        text: &str,
        color: D::Color,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
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
        D: DrawTarget,
        D::Color: RgbColor,
    {
        // Clear the area
        Rectangle::new(Point::new(0, 0), Self::SIZE)
            .into_styled(PrimitiveStyle::with_fill(D::Color::BLACK))
            .draw(target)?;

        self.draw_shape(&mut target.cropped(&Rectangle::new(
            Point::new(Self::PADDING as _, Self::PADDING as _),
            Size::new(Self::PADDED_WIDTH, Self::PADDED_HEIGHT),
        )))
    }

    fn draw_shape<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget + OriginDimensions,
        D::Color: RgbColor,
    {
        let bbox = target.bounding_box();

        if DIGITS > 5 {
            bbox.into_styled(PrimitiveStyle::with_stroke(D::Color::RED, 2))
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
            .into_styled(PrimitiveStyle::with_fill(D::Color::RED))
            .draw(target)?;
        }

        let wm_text = if let Some(edges_count) = self.edges_count {
            let mut wm_text = [b'0'; DIGITS];
            to_str(edges_count / self.divider as u64, &mut wm_text);

            wm_text
        } else {
            [b'?'; DIGITS]
        };

        self.draw_text(
            target,
            Point::zero(),
            str::from_utf8(&wm_text).unwrap(),
            D::Color::WHITE,
        )?;

        Ok(())
    }

    fn draw_text<D>(
        &self,
        target: &mut D,
        position: Point,
        text: &str,
        color: D::Color,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
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

fn to_str(mut num: u64, buf: &mut [u8]) -> usize {
    let mut len = buf.len();

    if num == 0 {
        len -= 1;

        buf[len] = b'0';
    }

    while num > 0 {
        len -= 1;

        buf[len] = b'0' + (num % 10) as u8;

        num /= 10;
    }

    len
}
