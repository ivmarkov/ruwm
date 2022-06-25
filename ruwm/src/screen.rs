use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use embedded_graphics::prelude::RgbColor;

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::signal::asynch::{SendSyncSignalFamily, Signal};
use embedded_svc::unblocker::asynch::Unblocker;
use embedded_svc::utils::asynch::channel::adapt::merge;
use embedded_svc::utils::asynch::select::{select3, select4, Either3, Either4};
use embedded_svc::utils::asynch::signal::adapt::as_channel;

use crate::battery::BatteryState;
use crate::utils::{as_static_receiver, as_static_sender};
use crate::valve::ValveState;
use crate::water_meter::WaterMeterState;
use crate::water_meter_stats::WaterMeterStatsState;

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

pub struct Screen<S>
where
    S: SendSyncSignalFamily,
{
    button1_pressed_signal: S::Signal<()>,
    button2_pressed_signal: S::Signal<()>,
    button3_pressed_signal: S::Signal<()>,
    valve_state_signal: S::Signal<Option<ValveState>>,
    wm_state_signal: S::Signal<WaterMeterState>,
    wm_stats_state_signal: S::Signal<WaterMeterStatsState>,
    battery_state_signal: S::Signal<BatteryState>,
    draw_request_signal: S::Signal<DrawRequest>,
}

impl<S> Screen<S>
where
    S: SendSyncSignalFamily,
{
    pub fn new() -> Self {
        Self {
            button1_pressed_signal: S::Signal::new(),
            button2_pressed_signal: S::Signal::new(),
            button3_pressed_signal: S::Signal::new(),
            valve_state_signal: S::Signal::new(),
            wm_state_signal: S::Signal::new(),
            wm_stats_state_signal: S::Signal::new(),
            battery_state_signal: S::Signal::new(),
            draw_request_signal: S::Signal::new(),
        }
    }

    pub fn button1_pressed_sink<'a>(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.button1_pressed_signal)
    }

    pub fn button2_pressed_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.button2_pressed_signal)
    }

    pub fn button3_pressed_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.button3_pressed_signal)
    }

    pub fn valve_state_sink(&'static self) -> impl Sender<Data = Option<ValveState>> + 'static {
        as_channel(&self.valve_state_signal)
    }

    pub fn wm_state_sink(&'static self) -> impl Sender<Data = WaterMeterState> + 'static {
        as_channel(&self.wm_state_signal)
    }

    pub fn wm_stats_state_sink(
        &'static self,
    ) -> impl Sender<Data = WaterMeterStatsState> + 'static {
        as_channel(&self.wm_stats_state_signal)
    }

    pub fn battery_state_sink(&'static self) -> impl Sender<Data = BatteryState> + 'static {
        as_channel(&self.battery_state_signal)
    }

    pub async fn draw<D>(&'static self, display: D)
    where
        D: FlushableDrawTarget + Send + 'static,
        D::Color: RgbColor,
        D::Error: Debug,
    {
        run_draw(as_static_receiver(&self.draw_request_signal), display)
            .await
            .unwrap(); // TODO
    }

    pub async fn process(
        &'static self,
        valve_state: Option<ValveState>,
        wm_state: WaterMeterState,
        battery_state: BatteryState,
        draw_request_sink: impl Sender<Data = DrawRequest> + Send + 'static,
    ) {
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
            merge(
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
) {
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
                valve,
                ..screen_state
            },
            Either4::Third(wm) => DrawRequest { wm, ..screen_state },
            Either4::Fourth(battery) => DrawRequest {
                battery,
                ..screen_state
            },
        };

        if screen_state != draw_request {
            screen_state = draw_request;

            draw_request_sink.send(draw_request).await;
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
) -> Result<(), D::Error>
where
    U: Unblocker,
    R: Receiver<Data = DrawRequest>,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug + Send + 'static,
{
    let mut page_drawable = None;

    loop {
        let draw_request = draw_request_source.recv().await;

        let result = unblocker
            .unblock(move || draw(display, page_drawable, draw_request))
            .await?;

        display = result.0;
        page_drawable = result.1;
    }
}

pub async fn run_draw<R, D>(mut draw_request_source: R, mut display: D) -> Result<(), D::Error>
where
    R: Receiver<Data = DrawRequest>,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    let mut page_drawable = None;

    loop {
        let draw_request = draw_request_source.recv().await;

        let result = draw(display, page_drawable, draw_request)?;

        display = result.0;
        page_drawable = result.1;
    }
}

fn draw<D>(
    mut display: D,
    mut page_drawable: Option<PageDrawable>,
    draw_request: DrawRequest,
) -> Result<(D, Option<PageDrawable>), D::Error>
where
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    loop {
        match draw_request.active_page {
            Page::Summary => {
                if let Some(PageDrawable::Summary(drawable)) = &mut page_drawable {
                    drawable.draw(
                        &mut display,
                        draw_request.valve,
                        draw_request.wm,
                        draw_request.battery,
                    )?;
                    break;
                } else {
                    page_drawable = Some(PageDrawable::Summary(pages::Summary::new()));
                }
            }
            Page::Battery => {
                if let Some(PageDrawable::Battery(drawable)) = &mut page_drawable {
                    drawable.draw(&mut display, draw_request.battery)?;
                    break;
                } else {
                    page_drawable = Some(PageDrawable::Battery(pages::Battery::new()));
                }
            }
        }

        display.clear(RgbColor::BLACK)?;
    }

    display.flush()?;

    Ok((display, page_drawable))
}
