pub use battery::*;
use embedded_graphics::{
    draw_target::Cropped,
    prelude::{DrawTarget, DrawTargetExt, Size},
    primitives::Rectangle,
};
pub use summary::*;

use super::{shapes::Textbox, Color};

pub mod actions;
mod battery;
mod summary;

pub fn with_title<'a, T>(
    target: &'a mut T,
    page_changed: bool,
    title: &str,
) -> Result<Cropped<'a, T>, T::Error>
where
    T: DrawTarget<Color = Color>,
{
    let padding = 2;

    let bbox = target.bounding_box();

    let Size { width, .. } = bbox.size;

    let main_font = if width <= 128 {
        profont::PROFONT_12_POINT
    } else {
        profont::PROFONT_18_POINT
    };

    let title_shape = Textbox {
        text: title,
        font: main_font,
        padding: 1,
        outline: 0,
        strikethrough: false,
        ..Default::default()
    };

    if page_changed {
        title_shape.draw(&mut target.cropped(&Rectangle::new(
            bbox.top_left + Size::new(padding, padding),
            Size::new(
                bbox.size.width - padding * 2,
                title_shape.preferred_size().height,
            ),
        )))?;
    }

    let padding = Size::new(2 as _, 2 as _);

    Ok(target.cropped(&Rectangle::new(
        bbox.top_left + padding + Size::new(0, title_shape.preferred_size().height),
        bbox.size - padding * 2 - Size::new(0, title_shape.preferred_size().height),
    )))
}
