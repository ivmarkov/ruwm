use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::prelude::{OriginDimensions, Point, Size};
use embedded_graphics::primitives::Rectangle;

use super::util::{clear, fill};
use super::Color;

#[derive(Clone, Debug)]
pub struct Valve {
    pub size: Size,
    pub padding: u32,
    pub outline: u32,
    pub handle_area: Size,
    pub open_percentage: Option<u8>,
}

impl Default for Valve {
    fn default() -> Self {
        Self::new()
    }
}

impl Valve {
    pub const fn new() -> Self {
        Self {
            size: Size::new(80, 60),
            padding: 10,
            outline: 4,
            handle_area: Size::new(20, 10),
            open_percentage: Some(100),
        }
    }

    pub fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        // Clear the area
        clear(&Rectangle::new(Point::new(0, 0), self.size), target)?;

        self.draw_shape(&mut target.cropped(&Rectangle::new(
            Point::new(self.padding as _, self.padding as _),
            self.padded_size(),
        )))
    }

    fn draw_shape<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color> + OriginDimensions,
    {
        let Size { width, height } = target.size();

        let percentage = self.open_percentage.unwrap_or(0);

        // let fill_line = if percentage >= 100 {
        //     0
        // } else {
        //     height * (100 - percentage as u32) / 100
        // };

        let water_color = Color::LightBlue;
        let outline_color = Color::Gray;

        // Draw water fill
        fill(
            &Rectangle::new(Point::new(0, 0), Size::new(width, height)),
            water_color,
            target,
        )?;

        // // Left outline
        // clear(
        //     &Rectangle::new(
        //         Point::new(0, self.handle_area.height as _),
        //         Size::new(self.outline, height - self.handle_area.height),
        //     ),
        //     target,
        // )?;

        // // Right outline
        // clear(
        //     &Rectangle::new(
        //         Point::new(
        //             width as i32 - self.outline as i32,
        //             self.handle_area.height as _,
        //         ),
        //         Size::new(self.outline, height - self.handle_area.height),
        //     ),
        //     target,
        // )?;

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

    fn padded_size(&self) -> Size {
        Size::new(
            self.size.width - self.padding * 2,
            self.size.height - self.padding * 2,
        )
    }
}
