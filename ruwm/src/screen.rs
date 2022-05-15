use core::fmt::Debug;

use embedded_svc::utils::asyncs::channel::adapt::both;
use serde::{Deserialize, Serialize};

use embedded_graphics::prelude::RgbColor;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::MutexFamily;
use embedded_svc::signal::asyncs::{SendSyncSignalFamily, Signal};
use embedded_svc::unblocker::asyncs::Unblocker;
use embedded_svc::utils::asyncs::select::{select3, select4, Either3, Either4};
use embedded_svc::utils::asyncs::signal::adapt::as_sender;

use crate::battery::BatteryState;
use crate::error;
use crate::utils::{as_static_receiver, as_static_sender};
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

pub struct Screen<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    button1_pressed_signal: M::Signal<()>,
    button2_pressed_signal: M::Signal<()>,
    button3_pressed_signal: M::Signal<()>,
    valve_state_signal: M::Signal<Option<ValveState>>,
    wm_state_signal: M::Signal<WaterMeterState>,
    battery_state_signal: M::Signal<BatteryState>,
    draw_request_signal: M::Signal<DrawRequest>,
}

impl<M> Screen<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    pub fn new() -> Self {
        Self {
            button1_pressed_signal: M::Signal::new(),
            button2_pressed_signal: M::Signal::new(),
            button3_pressed_signal: M::Signal::new(),
            valve_state_signal: M::Signal::new(),
            wm_state_signal: M::Signal::new(),
            battery_state_signal: M::Signal::new(),
            draw_request_signal: M::Signal::new(),
        }
    }

    pub fn button1_pressed_sink<'a>(&'static self) -> impl Sender<Data = ()> + 'static {
        as_sender(&self.button1_pressed_signal)
    }

    pub fn button2_pressed_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_sender(&self.button2_pressed_signal)
    }

    pub fn button3_pressed_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_sender(&self.button3_pressed_signal)
    }

    pub fn valve_state_sink(&'static self) -> impl Sender<Data = Option<ValveState>> + 'static {
        as_sender(&self.valve_state_signal)
    }

    pub fn wm_state_sink(&'static self) -> impl Sender<Data = WaterMeterState> + 'static {
        as_sender(&self.wm_state_signal)
    }

    pub fn battery_state_sink(&'static self) -> impl Sender<Data = BatteryState> + 'static {
        as_sender(&self.battery_state_signal)
    }

    pub async fn draw<D>(&'static self, display: D) -> error::Result<()>
    where
        D: FlushableDrawTarget + Send + 'static,
        D::Color: RgbColor,
        D::Error: Debug,
    {
        run_draw(as_static_receiver(&self.draw_request_signal), display).await
    }

    pub async fn process(
        &'static self,
        valve_state: Option<ValveState>,
        wm_state: WaterMeterState,
        battery_state: BatteryState,
        draw_request_sink: impl Sender<Data = DrawRequest> + Send + 'static,
    ) -> error::Result<()> {
        process(
            as_static_receiver(&self.button1_pressed_signal),
            as_static_receiver(&self.button2_pressed_signal),
            as_static_receiver(&self.button3_pressed_signal),
            as_static_receiver(&self.valve_state_signal),
            as_static_receiver(&self.wm_state_signal),
            as_static_receiver(&self.battery_state_signal),
            valve_state,
            wm_state,
            battery_state,
            both(
                as_static_sender(&self.draw_request_signal),
                draw_request_sink,
            ),
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn process(
    mut button1_pressed_source: impl Receiver<Data = ()>,
    mut button2_pressed_source: impl Receiver<Data = ()>,
    mut button3_pressed_source: impl Receiver<Data = ()>,
    mut valve_state_source: impl Receiver<Data = Option<ValveState>>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut battery_state_source: impl Receiver<Data = BatteryState>,
    valve_state: Option<ValveState>,
    wm_state: WaterMeterState,
    battery_state: BatteryState,
    mut draw_request_sink: impl Sender<Data = DrawRequest>,
) -> error::Result<()> {
    let mut screen_state = DrawRequest {
        active_page: Page::Summary,
        valve: valve_state,
        wm: wm_state,
        battery: battery_state,
    };

    loop {
        let button1_command = button1_pressed_source.recv();
        let button2_command = button2_pressed_source.recv();
        let button3_command = button3_pressed_source.recv();
        let valve = valve_state_source.recv();
        let wm = wm_state_source.recv();
        let battery = battery_state_source.recv();

        //pin_mut!(button1_command, button2_command, button3_command, valve, wm, battery);

        let draw_request = match select4(
            select3(button1_command, button2_command, button3_command),
            valve,
            wm,
            battery,
        )
        .await
        {
            Either4::First(Either3::First(_)) => DrawRequest {
                active_page: screen_state.active_page.prev(),
                ..screen_state
            },
            Either4::First(Either3::Second(_)) => DrawRequest {
                active_page: screen_state.active_page.next(),
                ..screen_state
            },
            Either4::First(Either3::Third(_)) => DrawRequest {
                active_page: screen_state.active_page,
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

            draw_request_sink
                .send(draw_request)
                .await
                .map_err(error::svc)?;
        }
    }
}

enum PageDrawable {
    Summary(pages::Summary),
    Battery(pages::Battery),
}

pub async fn unblock_run_draw<U, R, D>(
    unblocker: U,
    mut draw_request_source: R,
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
        let draw_request = draw_request_source.recv().await.map_err(error::svc)?;

        let result = unblocker
            .unblock(move || draw(display, page_drawable, draw_request))
            .await?;

        display = result.0;
        page_drawable = result.1;
    }
}

pub async fn run_draw<R, D>(mut draw_request_source: R, mut display: D) -> error::Result<()>
where
    R: Receiver<Data = DrawRequest>,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    let mut page_drawable = None;

    loop {
        let draw_request = draw_request_source.recv().await.map_err(error::svc)?;

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
