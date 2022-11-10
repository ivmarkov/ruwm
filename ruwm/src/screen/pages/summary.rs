use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::{Point, Size},
};

use crate::screen::shapes::{self, BatteryChargedText};
use crate::screen::{DrawTargetRef, RotateAngle, TransformingAdaptor};
use crate::valve::ValveState;
use crate::wm::WaterMeterState;
use crate::{battery::BatteryState, screen::shapes::Color};

pub struct Summary;

impl Summary {
    pub fn draw<D>(
        target: &mut D,
        valve_state: Option<&Option<ValveState>>,
        water_meter_state: Option<&WaterMeterState>,
        battery_state: Option<&BatteryState>,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        let bbox = target.bounding_box();

        if let Some(valve_state) = valve_state {
            let valve = shapes::Valve {
                open_percentage: valve_state.map(|valve_state| valve_state.open_percentage()),
                ..Default::default()
            };

            valve.draw(target)?;
        }

        if let Some(water_meter_state) = water_meter_state {
            let wm_shape = shapes::WaterMeterClassic::<8> {
                edges_count: Some(water_meter_state.edges_count),
                ..Default::default()
            };

            let mut target = TransformingAdaptor::display(DrawTargetRef::new(target)).translate(
                Point::new(bbox.size.width as i32 - wm_shape.size().width as i32, 22),
            );

            wm_shape.draw(&mut target)?;
        }

        if let Some(battery_state) = battery_state {
            let battery_shape = shapes::Battery {
                charged_percentage: battery_state.percentage(),
                text: BatteryChargedText::No,
                distinct_outline: false,
                ..Default::default()
            };

            let mut target = TransformingAdaptor::display(DrawTargetRef::new(target))
                .translate(Point::new(bbox.size.width as i32 - 40, -40))
                .scale_from(battery_shape.size, Size::new(20, 40))
                .rotate(RotateAngle::Degrees270);

            battery_shape.draw(&mut target)?;
        }

        Ok(())
    }
}
