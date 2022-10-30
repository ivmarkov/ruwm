use core::str;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::mono_font::*;
use embedded_graphics::prelude::{OriginDimensions, Point, Size};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::text::{Alignment, Baseline, TextStyleBuilder};

use super::util::{clear, fill, text, to_str};
use super::Color;

pub struct Valve {
    open_percentage: Option<u8>,
}

impl Valve {
    pub const SIZE: Size = Size::new(Self::WIDTH, Self::HEIGHT);
    pub const WIDTH: u32 = 80;
    pub const HEIGHT: u32 = 60;

    const FONT: MonoFont<'static> = profont::PROFONT_24_POINT;

    const PADDING: u32 = 10;
    const PADDED_WIDTH: u32 = Self::WIDTH - Self::PADDING * 2;
    const PADDED_HEIGHT: u32 = Self::HEIGHT - Self::PADDING * 2;

    const OUTLINE: u32 = 4;

    const HANDLE_AREA_WIDTH: u32 = 20;
    const HANDLE_AREA__HEIGHT: u32 = 10;

    const PERCENTAGE_THRESHOLD: u8 = 15;

    pub fn new(open_percentage: Option<u8>) -> Self {
        Self { open_percentage }
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
        //         Point::new(0, Self::HANDLE_AREA__HEIGHT as _),
        //         Size::new(Self::OUTLINE, height - Self::HANDLE_AREA__HEIGHT),
        //     ),
        //     target,
        // )?;

        // // Right outline
        // clear(
        //     &Rectangle::new(
        //         Point::new(
        //             width as i32 - Self::OUTLINE as i32,
        //             Self::HANDLE_AREA__HEIGHT as _,
        //         ),
        //         Size::new(Self::OUTLINE, height - Self::HANDLE_AREA__HEIGHT),
        //     ),
        //     target,
        // )?;

        // Bottom outline
        fill(
            &Rectangle::new(
                Point::new(0, height as i32 - Self::OUTLINE as i32),
                Size::new(width, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Handle
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 - Self::HANDLE_AREA_WIDTH as i32) / 2,
                    -(Self::OUTLINE as i32) * 2,
                ),
                Size::new(Self::HANDLE_AREA_WIDTH, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 - Self::OUTLINE as i32) / 2,
                    -(Self::OUTLINE as i32),
                ),
                Size::new(Self::OUTLINE, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top outline
        fill(
            &Rectangle::new(
                Point::new((width as i32 - Self::HANDLE_AREA_WIDTH as i32) / 2, 0),
                Size::new(Self::HANDLE_AREA_WIDTH, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top left horizontal outline
        fill(
            &Rectangle::new(
                Point::new(0, Self::HANDLE_AREA__HEIGHT as _),
                Size::new((width - Self::HANDLE_AREA_WIDTH) / 2, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top right horizontal outline
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 + Self::HANDLE_AREA_WIDTH as i32) / 2,
                    Self::HANDLE_AREA__HEIGHT as _,
                ),
                Size::new((width - Self::HANDLE_AREA_WIDTH) / 2, Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top left vertical outline
        fill(
            &Rectangle::new(
                Point::new((width as i32 - Self::HANDLE_AREA_WIDTH as i32) / 2, 0),
                Size::new(Self::OUTLINE, Self::HANDLE_AREA__HEIGHT + Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Top right vertical outline
        fill(
            &Rectangle::new(
                Point::new(
                    (width as i32 + Self::HANDLE_AREA_WIDTH as i32) / 2 - Self::OUTLINE as i32,
                    0,
                ),
                Size::new(Self::OUTLINE, Self::HANDLE_AREA__HEIGHT + Self::OUTLINE),
            ),
            outline_color,
            target,
        )?;

        // Remove fill from the top left corner
        clear(
            &Rectangle::new(
                Point::new(0, 0),
                Size::new(
                    (width - Self::HANDLE_AREA_WIDTH) / 2,
                    Self::HANDLE_AREA__HEIGHT,
                ),
            ),
            target,
        )?;

        // Remove fill from the top right corner
        clear(
            &Rectangle::new(
                Point::new((width as i32 + Self::HANDLE_AREA_WIDTH as i32) / 2, 0),
                Size::new(
                    (width - Self::HANDLE_AREA_WIDTH) / 2,
                    Self::HANDLE_AREA__HEIGHT,
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
