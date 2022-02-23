use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;

extern crate alloc;
use alloc::boxed::Box;

use anyhow::anyhow;

use futures::{pin_mut, select, FutureExt};

use embedded_graphics::prelude::RgbColor;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::unblocker::nonblocking::Unblocker;

use crate::battery::BatteryState;
use crate::button::ButtonCommand;
use crate::valve::ValveState;
use crate::water_meter::WaterMeterState;

pub use adaptors::*;

mod adaptors;
mod pages;
mod shapes;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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
    valve_state: Option<ValveState>,
    water_meter_state: WaterMeterState,
    battery_state: BatteryState,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_screen(
    mut button_command: impl Receiver<Data = ButtonCommand>,
    mut valve_state_updates: impl Receiver<Data = Option<ValveState>>,
    mut water_meter_state_updates: impl Receiver<Data = WaterMeterState>,
    mut battery_meter_state_updates: impl Receiver<Data = BatteryState>,
    valve_state: Option<ValveState>,
    water_meter_state: WaterMeterState,
    battery_state: BatteryState,
    mut draw_engine: impl Sender<Data = DrawRequest>,
) -> anyhow::Result<()> {
    let mut screen_state = DrawRequest {
        active_page: Page::Summary,
        valve_state,
        water_meter_state,
        battery_state,
    };

    loop {
        let command = button_command.recv().fuse();
        let valve_state_updates = valve_state_updates.recv().fuse();
        let water_meter_state_updates = water_meter_state_updates.recv().fuse();
        let battery_meter_state_updates = battery_meter_state_updates.recv().fuse();

        pin_mut!(command);
        pin_mut!(valve_state_updates);
        pin_mut!(water_meter_state_updates);
        pin_mut!(battery_meter_state_updates);

        let draw_request = select! {
            command = command => DrawRequest {
                active_page: match command.map_err(|e| anyhow!(e))? {
                    ButtonCommand::Pressed(1) => screen_state.active_page.prev(),
                    ButtonCommand::Pressed(2) => screen_state.active_page.next(),
                    ButtonCommand::Pressed(3) => screen_state.active_page,
                    _ => panic!("What's that button?"),
                },
                ..screen_state
            },
            valve_state = valve_state_updates => DrawRequest {
                valve_state: valve_state.map_err(|e| anyhow!(e))?,
                ..screen_state
            },
            water_meter_state = water_meter_state_updates => DrawRequest {
                water_meter_state: water_meter_state.map_err(|e| anyhow!(e))?,
                ..screen_state
            },
            battery_state = battery_meter_state_updates => DrawRequest {
                battery_state: battery_state.map_err(|e| anyhow!(e))?,
                ..screen_state
            }
        };

        if screen_state != draw_request {
            screen_state = draw_request;

            draw_engine
                .send(draw_request)
                .await
                .map_err(|e| anyhow!(e))?;
        }
    }
}

enum PageDrawable {
    Summary(pages::Summary),
    Battery(pages::Battery),
}

pub async fn run_draw_engine<U, D>(
    mut draw_notif: impl Receiver<Data = DrawRequest>,
    mut display: D,
) -> anyhow::Result<()>
where
    U: Unblocker,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    let mut page_drawable = None;

    loop {
        let draw_request = draw_notif.recv().await.map_err(|e| anyhow!(e))?;

        let result =
            U::unblock(Box::new(move || draw(display, page_drawable, draw_request))).await?;

        display = result.0;
        page_drawable = result.1;
    }
}

fn draw<D>(
    mut display: D,
    mut page_drawable: Option<PageDrawable>,
    draw_request: DrawRequest,
) -> anyhow::Result<(D, Option<PageDrawable>)>
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
                            draw_request.valve_state,
                            draw_request.water_meter_state,
                            draw_request.battery_state,
                        )
                        .map_err(|e| anyhow!("Display error: {:?}", e))?;
                    break;
                } else {
                    page_drawable = Some(PageDrawable::Summary(pages::Summary::new()));
                }
            }
            Page::Battery => {
                if let Some(PageDrawable::Battery(drawable)) = &mut page_drawable {
                    drawable
                        .draw(&mut display, draw_request.battery_state)
                        .map_err(|e| anyhow!("Display error: {:?}", e))?;
                    break;
                } else {
                    page_drawable = Some(PageDrawable::Battery(pages::Battery::new()));
                }
            }
        }

        display
            .clear(RgbColor::BLACK)
            .map_err(|e| anyhow!("Display error: {:?}", e))?;
    }

    display
        .flush()
        .map_err(|e| anyhow!("Display error: {:?}", e))?;

    Ok((display, page_drawable))
}
