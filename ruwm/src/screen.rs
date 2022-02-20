use core::fmt::{Debug, Display};
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

pub struct Screen<B, VU, WU, BU, D> {
    draw_request: DrawRequest,
    button_command: B,
    valve_state_updates: VU,
    water_meter_state_updates: WU,
    battery_meter_state_updates: BU,
    draw_engine: D,
}

impl<B, VU, WU, BU, D> Screen<B, VU, WU, BU, D>
where
    B: Receiver<Data = ButtonCommand>,
    VU: Receiver<Data = Option<ValveState>>,
    WU: Receiver<Data = WaterMeterState>,
    BU: Receiver<Data = BatteryState>,
    D: Sender<Data = DrawRequest>,
    B::Error: Send + Sync + Display + Debug + 'static,
    VU::Error: Send + Sync + Display + Debug + 'static,
    WU::Error: Send + Sync + Display + Debug + 'static,
    BU::Error: Send + Sync + Display + Debug + 'static,
    D::Error: Send + Sync + Display + Debug + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        button_command: B,
        valve_state_updates: VU,
        water_meter_state_updates: WU,
        battery_meter_state_updates: BU,
        valve_state: Option<ValveState>,
        water_meter_state: WaterMeterState,
        battery_state: BatteryState,
        draw_engine: D,
    ) -> Self {
        Self {
            button_command,
            valve_state_updates,
            water_meter_state_updates,
            battery_meter_state_updates,
            draw_request: DrawRequest {
                active_page: Page::Summary,
                valve_state,
                water_meter_state,
                battery_state,
            },
            draw_engine,
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let command = self.button_command.recv().fuse();
        let valve_state_updates = self.valve_state_updates.recv().fuse();
        let water_meter_state_updates = self.water_meter_state_updates.recv().fuse();
        let battery_meter_state_updates = self.battery_meter_state_updates.recv().fuse();

        pin_mut!(command);
        pin_mut!(valve_state_updates);
        pin_mut!(water_meter_state_updates);
        pin_mut!(battery_meter_state_updates);

        let draw_request = select! {
            command = command => DrawRequest {
                active_page: match command.map_err(|e| anyhow!(e))? {
                    ButtonCommand::Pressed(1) => self.draw_request.active_page.prev(),
                    ButtonCommand::Pressed(2) => self.draw_request.active_page.next(),
                    ButtonCommand::Pressed(3) => self.draw_request.active_page,
                    _ => panic!("What's that button?"),
                },
                ..self.draw_request
            },
            valve_state = valve_state_updates => DrawRequest {
                valve_state: valve_state.map_err(|e| anyhow!(e))?,
                ..self.draw_request
            },
            water_meter_state = water_meter_state_updates => DrawRequest {
                water_meter_state: water_meter_state.map_err(|e| anyhow!(e))?,
                ..self.draw_request
            },
            battery_state = battery_meter_state_updates => DrawRequest {
                battery_state: battery_state.map_err(|e| anyhow!(e))?,
                ..self.draw_request
            }
        };

        if self.draw_request != draw_request {
            self.draw_request = draw_request;

            self.draw_engine
                .send(draw_request)
                .await
                .map_err(|e| anyhow!(e))?;
        }

        Ok(())
    }
}

enum PageDrawable {
    Summary(pages::Summary),
    Battery(pages::Battery),
}

pub struct DrawEngine<U, N, D> {
    _unblocker: PhantomData<U>,
    display: Option<D>,
    page_drawable: Option<PageDrawable>,
    draw_notif: N,
}

impl<U, N, D> DrawEngine<U, N, D>
where
    U: Unblocker,
    N: Receiver<Data = DrawRequest>,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    N::Error: Debug + Display + Send + Sync + 'static,
    D::Error: Debug,
{
    pub fn new(draw_notif: N, display: D) -> Self {
        Self {
            _unblocker: PhantomData,
            display: Some(display),
            page_drawable: None,
            draw_notif,
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let draw_request = self.draw_notif.recv().await.map_err(|e| anyhow!(e))?;

        let display = mem::replace(&mut self.display, None);
        let page_drawable = mem::replace(&mut self.page_drawable, None);

        let result = U::unblock(Box::new(move || {
            Self::draw(display.unwrap(), page_drawable, draw_request)
        }))
        .await?;

        self.display = Some(result.0);
        self.page_drawable = result.1;

        Ok(())
    }

    fn draw(
        mut display: D,
        mut page_drawable: Option<PageDrawable>,
        draw_request: DrawRequest,
    ) -> anyhow::Result<(D, Option<PageDrawable>)> {
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
}
