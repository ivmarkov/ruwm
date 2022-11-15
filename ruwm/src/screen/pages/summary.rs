use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::{DrawTargetExt, Point, Size},
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

        let mut target = target.cropped(&bbox);

        let Size { width, height } = bbox.size;

        let (status_font, main_font, status_height, status_padding) = if width <= 128 {
            (profont::PROFONT_10_POINT, profont::PROFONT_18_POINT, 12, 5)
        } else {
            (profont::PROFONT_14_POINT, profont::PROFONT_24_POINT, 20, 2)
        };

        let mut offset: i32 = 0;

        let status_wifi = shapes::Wifi {
            size: Size::new(status_height * 3 / 4, status_height),
            padding: 1,
            outline: 1,
            strength: None, //Some(60),
            ..Default::default()
        };

        status_wifi.draw(&mut target)?;

        offset += (status_wifi.size.width + status_padding) as i32;

        let status_mqtt = shapes::Textbox {
            text: "MQTT",
            font: status_font,
            padding: 1,
            outline: 0,
            strikethrough: false,
            ..Default::default()
        };

        status_mqtt.draw(&mut target.translated(Point::new(offset, 0)))?;

        offset += (status_mqtt.size().width + status_padding) as i32;

        let status_power = shapes::Textbox {
            text: "PWR",
            color: Color::Green,
            font: status_font,
            padding: 1,
            outline: 0,
            strikethrough: false,
            ..Default::default()
        };

        if battery_state
            .and_then(|battery_state| battery_state.powered)
            .unwrap_or(false)
        {
            status_power.draw(&mut target.translated(Point::new(offset, 0)))?;
        }

        offset += (status_power.size().width + status_padding) as i32;

        let status_battery = shapes::Battery {
            size: Size::new(status_height, status_height * 2),
            charged_percentage: battery_state.and_then(|battery_state| battery_state.percentage()),
            text: BatteryChargedText::No,
            cathode: Size::new(status_height / 2, status_height / 4),
            padding: 1,
            outline: 1,
            distinct_outline: false,
            ..Default::default()
        };

        if battery_state.is_some() {
            status_battery.draw(
                &mut target
                    .translated(Point::new(offset, 0))
                    .rotated(RotateAngle::Degrees270),
            )?;
        }

        let mut y_offs = (status_height + status_padding) as i32;

        let wm_shape = shapes::WaterMeterClassic::<8> {
            edges_count: water_meter_state.map(|wm| wm.edges_count),
            font: main_font,
            ..Default::default()
        };

        wm_shape.draw(&mut target.translated(Point::new(
            ((width - wm_shape.size().width) / 2) as i32,
            y_offs,
        )))?;

        y_offs += (wm_shape.size().height + status_padding) as i32;

        let main_height = (height - status_height - status_padding) as i32 - y_offs;

        let valve_shape = shapes::Valve {
            size: Size::new(main_height as u32, main_height as u32),
            open_percentage: valve_state.and_then(|valve_state| {
                valve_state.map(|valve_state| valve_state.open_percentage())
            }),
            font: main_font,
            ..Default::default()
        };

        //valve_shape.draw(&mut target.translated(Point::new(0, y_offs)))?;

        Ok(())
    }
}
