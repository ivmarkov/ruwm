use embedded_graphics::pixelcolor::raw::RawU4;
use embedded_graphics::pixelcolor::{BinaryColor, Rgb565, Rgb888};
use embedded_graphics::prelude::{PixelColor, RgbColor};

pub use actions::*;
pub use battery::*;
pub use textbox::*;
pub use valve::*;
pub use wifi::*;
pub use wm::*;

mod actions;
mod battery;
mod textbox;
mod valve;
mod wifi;
mod wm;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Color {
    Black,
    Red,
    Blue,
    LightBlue,
    Gray,
    LightGray,
    Green,
    Yellow,
    White,
}

impl Color {
    pub fn into_rgb<C: RgbColor, F: Fn(u8, u8, u8) -> C>(self, converter: F) -> C {
        match self {
            Self::Black => C::BLACK,
            Self::Red => C::RED,
            Self::Blue => C::BLUE,
            Self::LightBlue => converter(50, 50, 200),
            Self::Gray => converter(128, 128, 128),
            Self::LightGray => converter(200, 200, 200),
            Self::Green => C::GREEN,
            Self::Yellow => C::YELLOW,
            Self::White => C::WHITE,
        }
    }

    pub fn into_binary(self) -> BinaryColor {
        if self.is_off() {
            BinaryColor::Off
        } else {
            BinaryColor::On
        }
    }

    pub fn is_off(&self) -> bool {
        matches!(self, Self::Black | Self::Gray)
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
            2 => Self::Blue,
            3 => Self::LightBlue,
            4 => Self::Gray,
            5 => Self::LightGray,
            6 => Self::Green,
            7 => Self::Yellow,
            _ => Self::White,
        }
    }
}

impl From<Color> for Rgb565 {
    fn from(color: Color) -> Self {
        color.into_rgb(Rgb565::new)
    }
}

impl From<Color> for Rgb888 {
    fn from(color: Color) -> Self {
        color.into_rgb(Rgb888::new)
    }
}

pub mod util {
    use embedded_graphics::draw_target::Cropped;
    use embedded_graphics::mono_font::{MonoFont, MonoTextStyleBuilder};
    use embedded_graphics::prelude::{DrawTarget, DrawTargetExt, Point, Size};
    use embedded_graphics::primitives::{Primitive, PrimitiveStyle, Rectangle};
    use embedded_graphics::text::{Alignment, Baseline, Text, TextStyle, TextStyleBuilder};
    use embedded_graphics::Drawable;

    use super::Color;

    pub fn clear<T>(area: &Rectangle, target: &mut T) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color>,
    {
        fill(area, Color::Black, target)
    }

    pub fn fill<T>(area: &Rectangle, color: T::Color, target: &mut T) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color>,
    {
        area.into_styled(PrimitiveStyle::with_fill(color))
            .draw(target)?;

        Ok(())
    }

    pub fn draw<T>(
        area: &Rectangle,
        color: T::Color,
        width: u32,
        target: &mut T,
    ) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color>,
    {
        area.into_styled(PrimitiveStyle::with_stroke(color, width))
            .draw(target)?;

        Ok(())
    }

    pub fn text<'a, T>(
        font: &'a MonoFont<'a>,
        target: &'a mut T,
        position: Point,
        text: &'a str,
        color: T::Color,
        text_style: Option<TextStyle>,
    ) -> Result<(), T::Error>
    where
        T: DrawTarget<Color = Color>,
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

    pub fn clear_cropped<'a, T>(target: &'a mut T, padding: u32) -> Result<Cropped<'a, T>, T::Error>
    where
        T: DrawTarget<Color = Color>,
    {
        let bbox = target.bounding_box();

        // Clear the area
        clear(&bbox, target)?;

        let padding = Size::new(padding as _, padding as _);

        Ok(target.cropped(&Rectangle::new(
            bbox.top_left + padding,
            bbox.size - padding * 2,
        )))
    }
}
