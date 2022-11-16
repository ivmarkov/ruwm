use core::{cmp::min, fmt::Write};

use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::{DrawTargetExt, Point, Size},
    primitives::Rectangle,
};

use crate::screen::{
    shapes::{self, BatteryChargedText},
    DrawTargetExt2,
};
use crate::valve::ValveState;
use crate::wm::WaterMeterState;
use crate::{battery::BatteryState, screen::shapes::Color};
use crate::{keepalive::RemainingTime, screen::RotateAngle};

pub struct Summary;

impl Summary {
    pub fn draw<D>(
        target: &mut D,
        valve_state: Option<&Option<ValveState>>,
        wm_state: Option<&WaterMeterState>,
        battery_state: Option<&BatteryState>,
        remaining_time_state: Option<&RemainingTime>,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Color>,
    {
        let bbox = target.bounding_box();

        let top_height = Self::draw_top_status_line(target, battery_state)?;
        let bottom_height = Self::draw_bottom_status_line(target, remaining_time_state)?;

        let content_rect = Rectangle::new(
            bbox.top_left + Size::new(0, top_height + 5),
            bbox.size - Size::new(0, top_height + bottom_height + 5),
        );

        Self::draw_content(&mut target.cropped(&content_rect), valve_state, wm_state)?;

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
            text: if battery_state
                .and_then(|battery_state| battery_state.powered)
                .unwrap_or(false)
            {
                "PWR"
            } else {
                "   "
            },
            color: Color::Green,
            font: status_font,
            padding: 1,
            outline: 0,
            strikethrough: false,
            ..Default::default()
        };

        x_right_offs -= status_power.preferred_size().width as i32;

        if battery_state.is_some() {
            status_power.draw(&mut target.cropped(&Rectangle::new(
                Point::new(x_right_offs, y_offs),
                status_power.preferred_size(),
            )))?;
        }

        Ok(status_height)
    }

    fn draw_content<D>(
        target: &mut D,
        valve_state: Option<&Option<ValveState>>,
        wm_state: Option<&WaterMeterState>,
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

        let mut y_offs = bbox.top_left.y;

        let wm_shape = shapes::WaterMeterClassic::<8> {
            edges_count: wm_state.map(|wm| wm.edges_count),
            font: main_font,
            ..Default::default()
        };

        if wm_state.is_some() {
            wm_shape.draw(&mut target.cropped(&Rectangle::new(
                Point::new(
                    ((width - wm_shape.preferred_size().width) / 2) as i32,
                    y_offs,
                ),
                wm_shape.preferred_size(),
            )))?;
        }

        y_offs += (wm_shape.preferred_size().height + 5) as i32;

        if valve_state.is_some() {
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
        }

        Ok(())
    }

    fn draw_bottom_status_line<D>(
        target: &mut D,
        remaining_time: Option<&RemainingTime>,
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

        if let Some(remaining_time) = remaining_time {
            let mut status_rt = shapes::Textbox {
                text: "            ",
                color: Color::Yellow,
                font: status_font,
                padding: 1,
                outline: 0,
                strikethrough: false,
                ..Default::default()
            };

            let status_rt_size = status_rt.preferred_size();

            let mut text_buf = heapless::String::<12>::new();
            status_rt.text = match remaining_time {
                RemainingTime::Indefinite => status_rt.text,
                RemainingTime::Duration(duration) => {
                    write!(&mut text_buf, "Sleep in {}s", min(duration.as_secs(), 99)).unwrap();

                    &text_buf
                }
            };

            status_rt.draw(&mut target.cropped(&Rectangle::new(
                bbox.top_left + Size::new(0, bbox.size.height - status_rt_size.height),
                status_rt_size,
            )))?;
        }

        Ok(status_height)
    }
}
