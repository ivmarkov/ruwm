use core::str;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::mono_font::*;
use embedded_graphics::prelude::{OriginDimensions, Point, Size};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Alignment, Baseline, TextStyleBuilder};

use super::util::{clear, clear_cropped, fill, text, to_str};
use super::Color;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BatteryChargedText {
    No,
    Outline,
    Xor,
}

#[derive(Clone, Debug)]
pub struct Battery<'a> {
    pub padding: u32,
    pub outline: u32,
    pub distinct_outline: bool,
    pub cathode: Size,
    pub percentage_threhsold: u8,
    pub text: BatteryChargedText,
    pub font: MonoFont<'a>,
    pub charged_percentage: Option<u8>,
}

impl<'a> Battery<'a> {
    pub const fn new() -> Self {
        Self {
            padding: 10,
            outline: 2,
            distinct_outline: true,
            cathode: Size::new(40, 10),
            percentage_threhsold: 15,
            text: BatteryChargedText::Xor,
            font: profont::PROFONT_24_POINT,
            charged_percentage: Some(100),
        }
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        self.draw_shape(&mut clear_cropped(target, self.padding)?)
    }

    fn draw_shape<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color> + OriginDimensions,
    {
        let Size { width, height } = target.size();

        let percentage = self.charged_percentage.unwrap_or(0);

        let fill_line = if percentage >= 100 {
            0
        } else {
            height * (100 - percentage as u32) / 100
        };

        let charged_color = if let Some(percentage) = self.charged_percentage {
            if percentage < self.percentage_threhsold {
                Color::Red
            } else {
                Color::Green
            }
        } else {
            Color::Yellow
        };

        let outline_color = if self.distinct_outline && self.charged_percentage.is_some() {
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
                Point::new(0, self.cathode.height as _),
                Size::new(self.outline, height - self.cathode.height),
            ),
            outline_color,
            target,
        )?;

        // Right outline
        fill(
            &Rectangle::new(
                Point::new(width as i32 - self.outline as i32, self.cathode.height as _),
                Size::new(self.outline, height - self.cathode.height),
            ),
            outline_color,
            target,
        )?;

        // Bottom outline
        fill(
            &Rectangle::new(
                Point::new(0, height as i32 - self.outline as i32),
                Size::new(width, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top outline
        fill(
            &Rectangle::new(
                Point::new((width as i32 - self.cathode.width as i32) / 2, 0),
                Size::new(self.cathode.width, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top left horizontal outline
        fill(
            &Rectangle::new(
                Point::new(0, self.cathode.height as _),
                Size::new((width - self.cathode.width) / 2, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top right horizontal outline
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 + self.cathode.width as i32) / 2,
                    self.cathode.height as _,
                ),
                Size::new((width - self.cathode.width) / 2, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top left vertical outline
        fill(
            &Rectangle::new(
                Point::new((width as i32 - self.cathode.width as i32) / 2, 0),
                Size::new(self.outline, self.cathode.height + self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top right vertical outline
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 + self.cathode.width as i32) / 2 - self.outline as i32,
                    0,
                ),
                Size::new(self.outline, self.cathode.height + self.outline),
            ),
            outline_color,
            target,
        )?;

        // Remove charge fill from the top left corner
        clear(
            &Rectangle::new(
                Point::new(0, 0),
                Size::new((width - self.cathode.width) / 2, self.cathode.height),
            ),
            target,
        )?;

        // Remove charge fill from the top right corner
        clear(
            &Rectangle::new(
                Point::new((width as i32 + self.cathode.width as i32) / 2, 0),
                Size::new((width - self.cathode.width) / 2, self.cathode.height),
            ),
            target,
        )?;

        let light_color = if percentage < self.percentage_threhsold {
            charged_color
        } else {
            outline_color
        };

        let position = Point::new(width as i32 / 2, height as i32 / 2);

        if self.text == BatteryChargedText::Xor {
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
        } else if self.text == BatteryChargedText::Outline {
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

        if let Some(percentage) = self.charged_percentage {
            let mut charged_text = [0_u8; 4];
            charged_text[3] = b'%';

            let offset = to_str(percentage as _, &mut charged_text[..3]);

            text(
                &self.font,
                target,
                position,
                str::from_utf8(&charged_text[offset..]).unwrap(),
                color,
                text_style,
            )
        } else {
            text(&self.font, target, position, "?", color, text_style)
        }
    }
}

impl<'a> Default for Battery<'a> {
    fn default() -> Self {
        Self::new()
    }
}
