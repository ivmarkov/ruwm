use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::{Point, RgbColor, Size},
};

use crate::{
    battery::BatteryState,
    screen::{
        shapes::{self, BatteryChargedText},
        DrawTargetRef, RotateAngle, TransformingAdaptor,
    },
    valve::ValveState,
    wm::WaterMeterState,
};

pub struct Summary;

impl Summary {
    pub fn draw<D>(
        target: &mut D,
        valve_state: Option<&Option<ValveState>>,
        water_meter_state: Option<&WaterMeterState>,
        battery_state: Option<&BatteryState>,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget,
        D::Color: RgbColor,
    {
        if let Some(valve_state) = valve_state {
            // TODO
        }

        if let Some(water_meter_state) = water_meter_state {
            let wm_shape =
                shapes::WaterMeterClassic::<8>::new(Some(water_meter_state.edges_count), 1, true);

            //let bbox = target.bounding_box();

            let mut target = TransformingAdaptor::display(DrawTargetRef::new(target))
                .translate(Point::new(0, 30));

            wm_shape.draw(&mut target)?;
        }

        if let Some(battery_state) = battery_state {
            let percentage = battery_state.voltage.map(|voltage| {
                (voltage as u32 * 100
                    / (BatteryState::MAX_VOLTAGE as u32 + BatteryState::LOW_VOLTAGE as u32))
                    as u8
            });

            let battery_shape = shapes::Battery::new(percentage, BatteryChargedText::No, false);

            let bbox = target.bounding_box();

            let mut target = TransformingAdaptor::display(DrawTargetRef::new(target))
                .translate(Point::new(bbox.size.width as i32 - 40, 0))
                .scale_from(shapes::Battery::SIZE, Size::new(20, 40))
                .rotate(RotateAngle::Degrees270);

            battery_shape.draw(&mut target)?;
        }

        Ok(())
    }
}
