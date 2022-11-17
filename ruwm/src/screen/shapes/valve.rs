use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::mono_font::MonoFont;
use embedded_graphics::prelude::{OriginDimensions, Point, Size};
use embedded_graphics::primitives::Rectangle;

use super::util::{clear, clear_cropped, fill, text};
use super::Color;

#[derive(Clone, Debug)]
pub struct Valve<'a> {
    pub font: MonoFont<'a>,
    pub padding: u32,
    pub outline: u32,
    pub handle_area: Size,
    pub open_percentage: Option<u8>,
}

impl<'a> Default for Valve<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Valve<'a> {
    pub const fn new() -> Self {
        Self {
            font: profont::PROFONT_18_POINT,
            padding: 10,
            outline: 4,
            handle_area: Size::new(20, 10),
            open_percentage: Some(100),
        }
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
        let Size { width, height } = target.size();

        let outline_color = Color::LightGray;

        if let Some(percentage) = self.open_percentage {
            let water_color = Color::LightBlue;

            // Draw water fill
            fill(
                &Rectangle::new(Point::new(0, 0), Size::new(width, height)),
                water_color,
                target,
            )?;

            const STEP: usize = 6;
            const BUBBLE: u32 = 1;

            for y in (0..height).step_by(STEP) {
                for x in (0..width).step_by(STEP) {
                    let x = x as i32;
                    let x_offs = if y as i32 % (STEP as i32 * 2) == 0 {
                        0
                    } else {
                        STEP as i32 / 2
                    };

                    if x + x_offs < width as i32 {
                        fill(
                            &Rectangle::new(
                                Point::new(x + x_offs, y as i32),
                                Size::new(BUBBLE, BUBBLE),
                            ),
                            Color::White,
                            target,
                        )?;
                    }
                }
            }

            let stop_line = if percentage >= 100 {
                0
            } else {
                height * (100 - percentage as u32) / 100
            };

            clear(
                &Rectangle::new(
                    Point::new((width / 2) as i32, 0),
                    Size::new(width / 2 + 1, stop_line),
                ),
                target,
            )?;

            fill(
                &Rectangle::new(
                    Point::new(((width - self.outline) / 2) as i32, 0),
                    Size::new(self.outline, stop_line),
                ),
                outline_color,
                target,
            )?;
        } else {
            text(
                &self.font,
                target,
                Point::new(
                    (width - self.font.character_size.width) as i32 / 2,
                    self.handle_area.height as i32
                        + (height - self.handle_area.height - self.font.character_size.height)
                            as i32
                            / 2,
                ),
                "?",
                Color::White,
                None,
            )?;
        }

        // Bottom outline
        fill(
            &Rectangle::new(
                Point::new(0, height as i32 - self.outline as i32),
                Size::new(width, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Handle
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 - self.handle_area.width as i32) / 2,
                    -(self.outline as i32) * 2,
                ),
                Size::new(self.handle_area.width, self.outline),
            ),
            outline_color,
            target,
        )?;

        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 - self.outline as i32) / 2,
                    -(self.outline as i32),
                ),
                Size::new(self.outline, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top outline
        fill(
            &Rectangle::new(
                Point::new((width as i32 - self.handle_area.width as i32) / 2, 0),
                Size::new(self.handle_area.width, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top left horizontal outline
        fill(
            &Rectangle::new(
                Point::new(0, self.handle_area.height as _),
                Size::new((width - self.handle_area.width) / 2, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top right horizontal outline
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 + self.handle_area.width as i32) / 2,
                    self.handle_area.height as _,
                ),
                Size::new((width - self.handle_area.width) / 2, self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top left vertical outline
        fill(
            &Rectangle::new(
                Point::new((width as i32 - self.handle_area.width as i32) / 2, 0),
                Size::new(self.outline, self.handle_area.height + self.outline),
            ),
            outline_color,
            target,
        )?;

        // Top right vertical outline
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 + self.handle_area.width as i32) / 2 - self.outline as i32,
                    0,
                ),
                Size::new(self.outline, self.handle_area.height + self.outline),
            ),
            outline_color,
            target,
        )?;

        // Remove fill from the top left corner
        clear(
            &Rectangle::new(
                Point::new(0, 0),
                Size::new(
                    (width - self.handle_area.width) / 2,
                    self.handle_area.height,
                ),
            ),
            target,
        )?;

        // Remove fill from the top right corner
        clear(
            &Rectangle::new(
                Point::new((width as i32 + self.handle_area.width as i32) / 2, 0),
                Size::new(
                    (width - self.handle_area.width) / 2,
                    self.handle_area.height,
                ),
            ),
            target,
        )?;

        // let light_color = if percentage < Self::PERCENTAGE_THRESHOLD {
        //     water_color
        // } else {
        //     outline_color
        // };

        Ok(())
    }
}
