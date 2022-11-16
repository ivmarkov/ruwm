use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::{DrawTargetExt, Point, Size},
    primitives::Rectangle,
};

use crate::screen::RotateAngle;
use crate::screen::{
    shapes::{self, BatteryChargedText},
    DrawTargetExt2,
};
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

        let Size { width, .. } = bbox.size;

        let main_font = if width <= 128 {
            profont::PROFONT_18_POINT
        } else {
            profont::PROFONT_24_POINT
        };

        let mut y_offs =
            bbox.top_left.y + Self::draw_top_status_line(target, battery_state)? as i32 + 5;

        let wm_shape = shapes::WaterMeterClassic::<8> {
            edges_count: water_meter_state.map(|wm| wm.edges_count),
            font: main_font,
            ..Default::default()
        };

        wm_shape.draw(&mut target.cropped(&Rectangle::new(
            Point::new(
                ((width - wm_shape.preferred_size().width) / 2) as i32,
                y_offs,
            ),
            wm_shape.preferred_size(),
        )))?;

        y_offs += (wm_shape.preferred_size().height + 5) as i32;

        let main_height = bbox.bottom_right().unwrap().x - y_offs;

        let valve_shape_size = Size::new(main_height as u32, main_height as u32);
        let valve_shape = shapes::Valve {
            open_percentage: valve_state.and_then(|valve_state| {
                valve_state.map(|valve_state| valve_state.open_percentage())
            }),
            font: main_font,
            ..Default::default()
        };

        valve_shape.draw(&mut target.cropped(&Rectangle::new(
            Point::new(bbox.top_left.x, y_offs),
            valve_shape_size,
        )))?;

        Ok(())
    }

    fn draw_top_status_line<D>(
        target: &mut D,
        battery_state: Option<&BatteryState>,
    ) -> Result<u32, D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        let bbox = target.bounding_box();

        let Size { width, .. } = bbox.size;

        let (status_font, status_height, status_padding) = if width <= 128 {
            (profont::PROFONT_10_POINT, 12, 5)
        } else {
            (profont::PROFONT_14_POINT, 20, 2)
        };

        let mut x_offs = bbox.top_left.x;
        let mut x_right_offs = bbox.bottom_right().unwrap().x;
        let y_offs = bbox.top_left.y;

        let status_wifi_size = Size::new(status_height * 3 / 4, status_height);
        let status_wifi = shapes::Wifi {
            padding: 1,
            outline: 1,
            strength: None, //Some(60),
            ..Default::default()
        };

        status_wifi.draw(&mut target.cropped(&Rectangle::new(
            Point::new(x_offs, y_offs),
            status_wifi_size,
        )))?;

        x_offs += (status_wifi_size.width + status_padding) as i32;

        let status_mqtt = shapes::Textbox {
            text: "MQTT",
            font: status_font,
            padding: 1,
            outline: 0,
            strikethrough: false,
            ..Default::default()
        };

        status_mqtt.draw(&mut target.cropped(&Rectangle::new(
            Point::new(x_offs, y_offs),
            status_mqtt.preferred_size(),
        )))?;

        //x_offs += (status_mqtt.preferred_size().width + status_padding) as i32;

        let status_battery_size = Size::new(status_height * 2, status_height);
        let status_battery = shapes::Battery {
            charged_percentage: battery_state.and_then(|battery_state| battery_state.percentage()),
            text: BatteryChargedText::No,
            cathode: Size::new(status_height / 2, status_height / 4),
            padding: 1,
            outline: 1,
            distinct_outline: false,
            ..Default::default()
        };

        x_right_offs -= status_battery_size.width as i32;

        if battery_state.is_some() {
            status_battery.draw(
                &mut target
                    .cropped(&Rectangle::new(
                        Point::new(x_right_offs, y_offs),
                        status_battery_size,
                    ))
                    .rotated(RotateAngle::Degrees270),
            )?;
        }

        x_right_offs -= status_padding as i32;

        let status_power = shapes::Textbox {
            text: "PWR",
            color: Color::Green,
            font: status_font,
            padding: 1,
            outline: 0,
            strikethrough: false,
            ..Default::default()
        };

        x_right_offs -= status_power.preferred_size().width as i32;

        if battery_state
            .and_then(|battery_state| battery_state.powered)
            .unwrap_or(false)
        {
            status_power.draw(&mut target.cropped(&Rectangle::new(
                Point::new(x_right_offs, y_offs),
                status_power.preferred_size(),
            )))?;
        }

        Ok(status_height)
    }
}
