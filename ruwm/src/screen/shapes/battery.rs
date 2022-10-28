use core::str;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::mono_font::*;
use embedded_graphics::prelude::{OriginDimensions, Point, Size};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Alignment, Baseline, TextStyleBuilder};

use super::util::{clear, fill, text, to_str};
use super::Color;

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

    const FONT: MonoFont<'static> = profont::PROFONT_24_POINT;

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
        let Size { width, height } = target.size();

        let percentage = self.percentage.unwrap_or(0);

        let fill_line = if percentage >= 100 {
            0
        } else {
            height * (100 - percentage as u32) / 100
        };

        let charged_color = if let Some(percentage) = self.percentage {
            if percentage < Self::PERCENTAGE_THRESHOLD {
                Color::Red
            } else {
                Color::Green
            }
        } else {
            Color::Yellow
        };

        let outline_color = if self.outline && self.percentage.is_some() {
            Color::White
        } else {
            charged_color
        };

        // Draw charging level fill
        fill(
            &Rectangle::new(
                Point::new(0, fill_line as _),
                Size::new(width, height - fill_line),
            ),
            charged_color,
            target,
        )?;

        // Left outline
        fill(
            &Rectangle::new(
                Point::new(0, Self::CATHODE_HEIGHT as _),
                Size::new(Self::OUTLINE, height - Self::CATHODE_HEIGHT),
            ),
            outline_color,
            target,
        )?;

        // Right outline
        fill(
            &Rectangle::new(
                Point::new(
                    width as i32 - Self::OUTLINE as i32,
                    Self::CATHODE_HEIGHT as _,
                ),
                Size::new(Self::OUTLINE, height - Self::CATHODE_HEIGHT),
            ),
            outline_color,
            target,
        )?;

        // Bottom outline
        fill(
            &Rectangle::new(
                Point::new(0, height as i32 - Self::OUTLINE as i32),
                Size::new(width, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top outline
        fill(
            &Rectangle::new(
                Point::new((width as i32 - Self::CATHODE_WIDTH as i32) / 2, 0),
                Size::new(Self::CATHODE_WIDTH, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top left horizontal outline
        fill(
            &Rectangle::new(
                Point::new(0, Self::CATHODE_HEIGHT as _),
                Size::new((width - Self::CATHODE_WIDTH) / 2, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top right horizontal outline
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 + Self::CATHODE_WIDTH as i32) / 2,
                    Self::CATHODE_HEIGHT as _,
                ),
                Size::new((width - Self::CATHODE_WIDTH) / 2, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top left vertical outline
        fill(
            &Rectangle::new(
                Point::new((width as i32 - Self::CATHODE_WIDTH as i32) / 2, 0),
                Size::new(Self::OUTLINE, Self::CATHODE_HEIGHT + Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top right vertical outline
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 + Self::CATHODE_WIDTH as i32) / 2 - Self::OUTLINE as i32,
                    0,
                ),
                Size::new(Self::OUTLINE, Self::CATHODE_HEIGHT + Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Remove charge fill from the top left corner
        clear(
            &Rectangle::new(
                Point::new(0, 0),
                Size::new((width - Self::CATHODE_WIDTH) / 2, Self::CATHODE_HEIGHT),
            ),
            target,
        )?;

        // Remove charge fill from the top right corner
        clear(
            &Rectangle::new(
                Point::new((width as i32 + Self::CATHODE_WIDTH as i32) / 2, 0),
                Size::new((width - Self::CATHODE_WIDTH) / 2, Self::CATHODE_HEIGHT),
            ),
            target,
        )?;

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
            self.draw_percentage(&mut bonw, position, Color::Black)?;
        } else if self.text_rendering == BatteryChargedText::Outline {
            self.draw_percentage(target, position, light_color)?;
        }

        Ok(())
    }

    fn draw_percentage<D>(
        &self,
        target: &mut D,
        position: Point,
        color: Color,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        let text_style = Some(
            TextStyleBuilder::new()
                .baseline(Baseline::Middle)
                .alignment(Alignment::Center)
                .build(),
        );

        if let Some(percentage) = self.percentage {
            let mut charged_text = [0_u8; 4];
            charged_text[3] = b'%';

            let offset = to_str(percentage as _, &mut charged_text[..3]);

            text(
                &Self::FONT,
                target,
                position,
                str::from_utf8(&charged_text[offset..]).unwrap(),
                color,
                text_style,
            )
        } else {
            text(&Self::FONT, target, position, "?", color, text_style)
        }
    }
}
