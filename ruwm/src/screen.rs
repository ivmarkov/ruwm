use core::fmt::Debug;

use embedded_svc::utils::asyncs::select::{select4, Either4};
use futures::pin_mut;

use serde::{Deserialize, Serialize};

use embedded_graphics::prelude::RgbColor;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::unblocker::asyncs::Unblocker;

use crate::battery::BatteryState;
use crate::button::ButtonCommand;
use crate::error;
use crate::valve::ValveState;
use crate::water_meter::WaterMeterState;

pub use adaptors::*;

mod adaptors;
mod pages;
mod shapes;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Page {
    Summary = 0,
    Battery = 1,
}

impl Page {
    pub fn prev(&self) -> Self {
        match self {
            Self::Summary => Self::Battery,
            Self::Battery => Self::Summary,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Summary => Self::Battery,
            Self::Battery => Self::Summary,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct DrawRequest {
    active_page: Page,
    valve: Option<ValveState>,
    wm: WaterMeterState,
    battery: BatteryState,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_screen(
    mut button_command: impl Receiver<Data = ButtonCommand>,
    mut valve: impl Receiver<Data = Option<ValveState>>,
    mut wm: impl Receiver<Data = WaterMeterState>,
    mut battery: impl Receiver<Data = BatteryState>,
    valve_state: Option<ValveState>,
    wm_state: WaterMeterState,
    battery_state: BatteryState,
    mut draw_engine: impl Sender<Data = DrawRequest>,
) -> error::Result<()> {
    let mut screen_state = DrawRequest {
        active_page: Page::Summary,
        valve: valve_state,
        wm: wm_state,
        battery: battery_state,
    };

    loop {
        let command = button_command.recv();
        let valve = valve.recv();
        let wm = wm.recv();
        let battery = battery.recv();

        pin_mut!(command, valve, wm, battery);

        let draw_request = match select4(command, valve, wm, battery).await {
            Either4::First(command) => DrawRequest {
                active_page: match command.map_err(error::svc)? {
                    ButtonCommand::Pressed(1) => screen_state.active_page.prev(),
                    ButtonCommand::Pressed(2) => screen_state.active_page.next(),
                    ButtonCommand::Pressed(3) => screen_state.active_page,
                    _ => panic!("What's that button?"),
                },
                ..screen_state
            },
            Either4::Second(valve) => DrawRequest {
                valve: valve.map_err(error::svc)?,
                ..screen_state
            },
            Either4::Third(wm) => DrawRequest {
                wm: wm.map_err(error::svc)?,
                ..screen_state
            },
            Either4::Fourth(battery) => DrawRequest {
                battery: battery.map_err(error::svc)?,
                ..screen_state
            },
        };

        if screen_state != draw_request {
            screen_state = draw_request;

            draw_engine.send(draw_request).await.map_err(error::svc)?;
        }
    }
}

enum PageDrawable {
    Summary(pages::Summary),
    Battery(pages::Battery),
}

pub async fn unblock_run_draw_engine<U, R, D>(
    unblocker: U,
    mut draw_notif: R,
    mut display: D,
) -> error::Result<()>
where
    U: Unblocker,
    R: Receiver<Data = DrawRequest>,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    let mut page_drawable = None;

    loop {
        let draw_request = draw_notif.recv().await.map_err(error::svc)?;

        let result = unblocker
            .unblock(move || draw(display, page_drawable, draw_request))
            .await?;

        display = result.0;
        page_drawable = result.1;
    }
}

pub async fn run_draw_engine<R, D>(mut draw_notif: R, mut display: D) -> error::Result<()>
where
    R: Receiver<Data = DrawRequest>,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    let mut page_drawable = None;

    loop {
        let draw_request = draw_notif.recv().await.map_err(error::svc)?;

        let result = draw(display, page_drawable, draw_request)?;

        display = result.0;
        page_drawable = result.1;
    }
}

fn draw<D>(
    mut display: D,
    mut page_drawable: Option<PageDrawable>,
    draw_request: DrawRequest,
) -> error::Result<(D, Option<PageDrawable>)>
where
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    loop {
        match draw_request.active_page {
            Page::Summary => {
                if let Some(PageDrawable::Summary(drawable)) = &mut page_drawable {
                    drawable
                        .draw(
                            &mut display,
                            draw_request.valve,
                            draw_request.wm,
                            draw_request.battery,
                        )
                        .map_err(error::debug)?;
                    break;
                } else {
                    page_drawable = Some(PageDrawable::Summary(pages::Summary::new()));
                }
            }
            Page::Battery => {
                if let Some(PageDrawable::Battery(drawable)) = &mut page_drawable {
                    drawable
                        .draw(&mut display, draw_request.battery)
                        .map_err(error::debug)?;
                    break;
                } else {
                    page_drawable = Some(PageDrawable::Battery(pages::Battery::new()));
                }
            }
        }

        display.clear(RgbColor::BLACK).map_err(error::debug)?;
    }

    display.flush().map_err(error::debug)?;

    Ok((display, page_drawable))
}
