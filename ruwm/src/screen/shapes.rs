use embedded_graphics::pixelcolor::raw::RawU4;
use embedded_graphics::prelude::{PixelColor, RgbColor};

pub use battery::*;
pub use water_meter::*;

mod battery;
mod water_meter;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    White,
}

impl Color {
    pub fn into_rgb<C: RgbColor>(self) -> C {
        match self {
            Color::Black => C::BLACK,
            Color::Red => C::RED,
            Color::Green => C::GREEN,
            Color::Yellow => C::YELLOW,
            Color::White => C::WHITE,
        }
    }
}

impl PixelColor for Color {
    type Raw = RawU4;
}

impl From<Color> for RawU4 {
    fn from(color: Color) -> Self {
        (color as u8).into()
    }
}

impl From<u8> for Color {
    fn from(raw: u8) -> Self {
        match raw {
            0 => Self::Black,
            1 => Self::Red,
            2 => Self::Green,
            3 => Self::Yellow,
            _ => Self::White,
        }
    }
}

pub mod util {
    use embedded_graphics::mono_font::{MonoFont, MonoTextStyleBuilder};
    use embedded_graphics::prelude::{DrawTarget, Point};
    use embedded_graphics::primitives::{Primitive, PrimitiveStyle, Rectangle};
    use embedded_graphics::text::{Alignment, Baseline, Text, TextStyle, TextStyleBuilder};
    use embedded_graphics::Drawable;

    use super::Color;

    pub fn clear<D>(area: &Rectangle, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        fill(area, Color::Black, target)
    }

    pub fn fill<D>(area: &Rectangle, color: D::Color, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        area.into_styled(PrimitiveStyle::with_fill(color))
            .draw(target)?;

        Ok(())
    }

    pub fn draw<D>(
        area: &Rectangle,
        color: D::Color,
        width: u32,
        target: &mut D,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        area.into_styled(PrimitiveStyle::with_stroke(color, width))
            .draw(target)?;

        Ok(())
    }

    pub fn text<'a, D>(
        font: &'a MonoFont<'a>,
        target: &'a mut D,
        position: Point,
        text: &'a str,
        color: D::Color,
        text_style: Option<TextStyle>,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        let character_style = MonoTextStyleBuilder::new()
            .font(font)
            .text_color(color)
            .build();

        let text_style = text_style.unwrap_or_else(|| {
            TextStyleBuilder::new()
                .baseline(Baseline::Top)
                .alignment(Alignment::Left)
                .build()
        });

        Text::with_text_style(text, position, character_style, text_style).draw(target)?;

        Ok(())
    }

    pub fn to_str(mut num: u64, buf: &mut [u8]) -> usize {
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
}
