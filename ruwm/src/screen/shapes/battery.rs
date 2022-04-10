use core::str;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::prelude::{OriginDimensions, Point, Primitive, RgbColor, Size};
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};
use embedded_graphics::Drawable;

use profont::PROFONT_24_POINT;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BatteryChargedText {
    No,
    Outline,
    Xor,
}

pub struct Battery {
    percentage: Option<u8>,
    text_rendering: BatteryChargedText,
    outline: bool,
}

impl Battery {
    pub const SIZE: Size = Size::new(Self::WIDTH, Self::HEIGHT);
    pub const WIDTH: u32 = 100;
    pub const HEIGHT: u32 = 200;

    const PADDING: u32 = 10;
    const PADDED_WIDTH: u32 = Self::WIDTH - Self::PADDING * 2;
    const PADDED_HEIGHT: u32 = Self::HEIGHT - Self::PADDING * 2;

    const OUTLINE: u32 = 5;

    const CATHODE_WIDTH: u32 = 40;
    const CATHODE_HEIGHT: u32 = 10;

    const PERCENTAGE_THRESHOLD: u8 = 15;

    pub fn new(percentage: Option<u8>, text_rendering: BatteryChargedText, outline: bool) -> Self {
        Self {
            percentage,
            text_rendering,
            outline,
        }
    }

    pub fn size(&self) -> Size {
        Size::new(
            Self::PADDED_WIDTH + Self::PADDING * 2,
            Self::PADDED_HEIGHT + Self::PADDING * 2,
        )
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
    {
        // Clear the area
        Rectangle::new(Point::new(0, 0), self.size())
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
        let Size { width, height } = target.size();

        let percentage = self.percentage.unwrap_or(0);

        let fill_line = if percentage >= 100 {
            0
        } else {
            height * (100 - percentage as u32) / 100
        };

        let charged_color = if let Some(percentage) = self.percentage {
            if percentage < Self::PERCENTAGE_THRESHOLD {
                D::Color::RED
            } else {
                D::Color::GREEN
            }
        } else {
            D::Color::YELLOW
        };

        let outline_color = if self.outline && self.percentage.is_some() {
            D::Color::WHITE
        } else {
            charged_color
        };

        // Draw charging level fill
        Rectangle::new(
            Point::new(0, fill_line as _),
            Size::new(width, height - fill_line),
        )
        .into_styled(PrimitiveStyle::with_fill(charged_color))
        .draw(target)?;

        // Left outline
        Rectangle::new(
            Point::new(0, Self::CATHODE_HEIGHT as _),
            Size::new(Self::OUTLINE, height - Self::CATHODE_HEIGHT),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Right outline
        Rectangle::new(
            Point::new(
                width as i32 - Self::OUTLINE as i32,
                Self::CATHODE_HEIGHT as _,
            ),
            Size::new(Self::OUTLINE, height - Self::CATHODE_HEIGHT),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Bottom outline
        Rectangle::new(
            Point::new(0, height as i32 - Self::OUTLINE as i32),
            Size::new(width, Self::OUTLINE),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Top outline
        Rectangle::new(
            Point::new((width as i32 - Self::CATHODE_WIDTH as i32) / 2, 0),
            Size::new(Self::CATHODE_WIDTH, Self::OUTLINE),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Top left horizontal outline
        Rectangle::new(
            Point::new(0, Self::CATHODE_HEIGHT as _),
            Size::new((width - Self::CATHODE_WIDTH) / 2, Self::OUTLINE),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Top right horizontal outline
        Rectangle::new(
            Point::new(
                (width as i32 + Self::CATHODE_WIDTH as i32) / 2,
                Self::CATHODE_HEIGHT as _,
            ),
            Size::new((width - Self::CATHODE_WIDTH) / 2, Self::OUTLINE),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Top left vertical outline
        Rectangle::new(
            Point::new((width as i32 - Self::CATHODE_WIDTH as i32) / 2, 0),
            Size::new(Self::OUTLINE, Self::CATHODE_HEIGHT + Self::OUTLINE),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Top right vertical outline
        Rectangle::new(
            Point::new(
                (width as i32 + Self::CATHODE_WIDTH as i32) / 2 - Self::OUTLINE as i32,
                0,
            ),
            Size::new(Self::OUTLINE, Self::CATHODE_HEIGHT + Self::OUTLINE),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Remove charge fill from the top left corner
        Rectangle::new(
            Point::new(0, 0),
            Size::new((width - Self::CATHODE_WIDTH) / 2, Self::CATHODE_HEIGHT),
        )
        .into_styled(PrimitiveStyle::with_fill(D::Color::BLACK))
        .draw(target)?;

        // Remove charge fill from the top right corner
        Rectangle::new(
            Point::new((width as i32 + Self::CATHODE_WIDTH as i32) / 2, 0),
            Size::new((width - Self::CATHODE_WIDTH) / 2, Self::CATHODE_HEIGHT),
        )
        .into_styled(PrimitiveStyle::with_fill(D::Color::BLACK))
        .draw(target)?;

        let light_color = if percentage < Self::PERCENTAGE_THRESHOLD {
            charged_color
        } else {
            outline_color
        };

        let position = Point::new(width as i32 / 2, height as i32 / 2);

        if self.text_rendering == BatteryChargedText::Xor {
            let mut wonb = target.clipped(&Rectangle::new(
                Point::new(0, 0),
                Size::new(width, fill_line),
            ));
            self.draw_percentage(&mut wonb, position, light_color)?;

            let mut bonw = target.clipped(&Rectangle::new(
                Point::new(0, fill_line as i32),
                Size::new(width, height - fill_line),
            ));
            self.draw_percentage(&mut bonw, position, D::Color::BLACK)?;
        } else if self.text_rendering == BatteryChargedText::Outline {
            self.draw_percentage(target, position, light_color)?;
        }

        Ok(())
    }

    fn draw_percentage<D>(
        &self,
        target: &mut D,
        position: Point,
        color: D::Color,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
    {
        if let Some(percentage) = self.percentage {
            let mut charged_text = [0_u8; 4];
            charged_text[3] = b'%';

            let offset = to_str(percentage as _, &mut charged_text[..3]);

            self.draw_text(
                target,
                position,
                str::from_utf8(&charged_text[offset..]).unwrap(),
                color,
            )
        } else {
            self.draw_text(target, position, "?", color)
        }
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
            .font(&PROFONT_24_POINT)
            .text_color(color)
            .build();

        let text_style = TextStyleBuilder::new()
            .baseline(Baseline::Middle)
            .alignment(Alignment::Center)
            .build();

        Text::with_text_style(text, position, character_style, text_style).draw(target)?;

        Ok(())
    }
}

fn to_str(mut num: u32, buf: &mut [u8]) -> usize {
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
