use embedded_graphics::{
    prelude::{DrawTarget, DrawTargetExt, Point, Size},
    primitives::Rectangle,
};
use enumset::EnumSet;

use crate::screen::{
    shapes::{Action, Actions},
    Color,
};

pub fn draw<T>(target: &mut T, actions: EnumSet<Action>, action: Action) -> Result<(), T::Error>
where
    T: DrawTarget<Color = Color>,
{
    let bbox = target.bounding_box();

    let Size { width, .. } = bbox.size;

    let font = if width <= 128 {
        profont::PROFONT_12_POINT
    } else {
        profont::PROFONT_24_POINT
    };

    let bbox = target.bounding_box();

    let actions_shape = Actions {
        enabled: actions,
        selected: action,
        font,
        ..Default::default()
    };

    let actions_shape_size = Size::new(bbox.size.width - 10, actions_shape.preferred_size().height);

    let mut target = target.cropped(&Rectangle::new(
        Point::new(
            (bbox.size.width as i32 - actions_shape_size.width as i32) / 2,
            (bbox.size.height as i32 - actions_shape_size.height as i32) / 2,
        ),
        actions_shape_size,
    ));

    actions_shape.draw(&mut target)?;

    Ok(())
}
