use core::str;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::prelude::{Point, Primitive, RgbColor, Size};
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};
use embedded_graphics::Drawable;

use profont::{PROFONT_12_POINT, PROFONT_24_POINT};

const WIDTH: u32 = 100;
const HEIGHT: u32 = 200;

const PERCENTAGE_THRESHOLD: u8 = 15;

pub struct Battery {
    percentage: Option<u8>,
    xor: bool,
    outline: bool,
}

impl Battery {
    pub fn new(percentage: Option<u8>, xor: bool, outline: bool) -> Self {
        Self {
            percentage,
            xor,
            outline,
        }
    }

    pub fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
    {
        // Clear the area
        Rectangle::new(Point::new(0, 0), Size::new(WIDTH, HEIGHT))
            .into_styled(PrimitiveStyle::with_fill(D::Color::BLACK))
            .draw(target)?;

        let percentage = self.percentage.unwrap_or(0);

        let fill_line = if percentage == 100 {
            10
        } else {
            (HEIGHT - 20) * 100 / (100 - percentage as u32)
        };

        let charged_color = if let Some(percentage) = self.percentage {
            if percentage < PERCENTAGE_THRESHOLD {
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
        Rectangle::new(Point::new(10, 10), Size::new(WIDTH - 20, HEIGHT - 20))
            .into_styled(PrimitiveStyle::with_fill(charged_color))
            .draw(target)?;

        // Left outline
        Rectangle::new(Point::new(10, 20), Size::new(5, HEIGHT - 20))
            .into_styled(PrimitiveStyle::with_fill(outline_color))
            .draw(target)?;

        // Right outline
        Rectangle::new(
            Point::new(WIDTH as i32 - 20 - 5, 20),
            Size::new(5, HEIGHT - 20),
        )
        .into_styled(PrimitiveStyle::with_fill(outline_color))
        .draw(target)?;

        // Bottom outline
        Rectangle::new(
            Point::new(10, HEIGHT as i32 - 20 - 5),
            Size::new(WIDTH - 20, 5),
        )
        .into_styled(PrimitiveStyle::with_stroke(outline_color, 5))
        .draw(target)?;

        // Top outline
        Rectangle::new(Point::new(40, 10), Size::new(WIDTH - 80, 5))
            .into_styled(PrimitiveStyle::with_stroke(outline_color, 5))
            .draw(target)?;

        // Top left outline
        Rectangle::new(Point::new(40, 10), Size::new(5, 10))
            .into_styled(PrimitiveStyle::with_stroke(outline_color, 5))
            .draw(target)?;

        // Top right outline
        Rectangle::new(Point::new(WIDTH as i32 - 40, 10), Size::new(5, 10))
            .into_styled(PrimitiveStyle::with_stroke(outline_color, 5))
            .draw(target)?;

        // Remove charge fill from the top left corner
        Rectangle::new(Point::new(10, 10), Size::new(40 - 5, 10 - 5))
            .into_styled(PrimitiveStyle::with_stroke(D::Color::BLACK, 5))
            .draw(target)?;

        // Remove charge fill from the top right corner
        Rectangle::new(
            Point::new(WIDTH as i32 - 40 + 5, 10),
            Size::new(40 - 5, 10 - 5),
        )
        .into_styled(PrimitiveStyle::with_stroke(D::Color::BLACK, 5))
        .draw(target)?;

        let light_color = if percentage < PERCENTAGE_THRESHOLD {
            charged_color
        } else {
            outline_color
        };

        if self.xor {
            let mut wonb = target.clipped(&Rectangle::new(
                Point::new(0, 0),
                Size::new(WIDTH, fill_line),
            ));
            self.draw_percentage(&mut wonb, light_color)?;

            let mut bonw = target.clipped(&Rectangle::new(
                Point::new(0, fill_line as i32),
                Size::new(WIDTH, HEIGHT),
            ));
            self.draw_percentage(&mut bonw, D::Color::BLACK)?;
        } else {
            self.draw_percentage(target, light_color)?;
        }

        Ok(())
    }

    fn draw_percentage<D>(&self, target: &mut D, color: D::Color) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
    {
        if let Some(percentage) = self.percentage {
            let charged_text = [
                if percentage < 100 { b' ' } else { b'1' },
                b'0' + percentage / 10,
                b'0' + percentage % 10,
                b'%',
            ];

            self.draw_text(target, str::from_utf8(&charged_text).unwrap(), color)
        } else {
            self.draw_text(target, "?", color)
        }
    }

    fn draw_text<D>(&self, target: &mut D, text: &str, color: D::Color) -> Result<(), D::Error>
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

        Text::with_text_style(
            text,
            Point::new(WIDTH as i32 / 2, HEIGHT as i32 / 2),
            character_style,
            text_style,
        )
        .draw(target)?;

        Ok(())
    }
}
