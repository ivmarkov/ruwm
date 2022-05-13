use core::fmt::Debug;

use futures::pin_mut;

use serde::{Deserialize, Serialize};

use embedded_graphics::prelude::RgbColor;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::MutexFamily;
use embedded_svc::unblocker::asyncs::Unblocker;
use embedded_svc::utils::asyncs::select::{select4, Either4, Either3, select3};
use embedded_svc::utils::asyncs::signal::adapt::{as_sender, as_receiver};
use embedded_svc::utils::asyncs::signal::{MutexSignal, State};

use crate::battery::BatteryState;
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

pub struct Screen<M> 
where 
    M: MutexFamily,
{
    button1_command: MutexSignal<M::Mutex<State<()>>, ()>,
    button2_command: MutexSignal<M::Mutex<State<()>>, ()>,
    button3_command: MutexSignal<M::Mutex<State<()>>, ()>,
    valve_notif: MutexSignal<M::Mutex<State<Option<ValveState>>>, Option<ValveState>>,
    wm_notif: MutexSignal<M::Mutex<State<WaterMeterState>>, WaterMeterState>,
    battery_notif: MutexSignal<M::Mutex<State<BatteryState>>, BatteryState>,
    draw_notif: MutexSignal<M::Mutex<State<DrawRequest>>, DrawRequest>,
}

impl<M> Screen<M> 
where 
    M: MutexFamily,
{
    pub fn new() -> Self {
        Self {
            button1_command: MutexSignal::new(),
            button2_command: MutexSignal::new(),
            button3_command: MutexSignal::new(),
            valve_notif: MutexSignal::new(),
            wm_notif: MutexSignal::new(),
            battery_notif: MutexSignal::new(),
            draw_notif: MutexSignal::new(),
        }
    }

    pub fn button1_command(&self) -> impl Sender<Data = ()> + '_ 
    where 
        M::Mutex<State<()>>: Send + Sync, 
    {
        as_sender(&self.button1_command)
    }

    pub fn button2_command(&self) -> impl Sender<Data = ()> + '_ 
    where 
        M::Mutex<State<()>>: Send + Sync, 
    {
        as_sender(&self.button2_command)
    }

    pub fn button3_command(&self) -> impl Sender<Data = ()> + '_ 
    where 
        M::Mutex<State<()>>: Send + Sync, 
    {
        as_sender(&self.button3_command)
    }

    pub fn valve_notif(&self) -> impl Sender<Data = Option<ValveState>> + '_ 
    where 
        M::Mutex<State<Option<ValveState>>>: Send + Sync, 
    {
        as_sender(&self.valve_notif)
    }

    pub fn wm_notif(&self) -> impl Sender<Data = WaterMeterState> + '_ 
    where 
        M::Mutex<State<WaterMeterState>>: Send + Sync, 
    {
        as_sender(&self.wm_notif)
    }

    pub fn battery_notif(&self) -> impl Sender<Data = BatteryState> + '_ 
    where 
        M::Mutex<State<BatteryState>>: Send + Sync, 
    {
        as_sender(&self.battery_notif)
    }

    pub async fn run_screen(
        &self,
        valve_state: Option<ValveState>,
        wm_state: WaterMeterState,
        battery_state: BatteryState,
    ) -> error::Result<()> 
    where 
        M::Mutex<State<()>>: Send + Sync, 
        M::Mutex<State<DrawRequest>>: Send + Sync, 
        M::Mutex<State<Option<ValveState>>>: Send + Sync, 
        M::Mutex<State<WaterMeterState>>: Send + Sync, 
        M::Mutex<State<BatteryState>>: Send + Sync, 
    {
        run_screen(
            as_receiver(&self.button1_command),
            as_receiver(&self.button2_command),
            as_receiver(&self.button3_command),
            as_receiver(&self.valve_notif),
            as_receiver(&self.wm_notif),
            as_receiver(&self.battery_notif),
            valve_state,
            wm_state,
            battery_state,
            as_sender(&self.draw_notif),
        ).await
    }

    pub async fn run_draw_engine<D>(&self, display: D) -> error::Result<()>
    where 
        M::Mutex<State<DrawRequest>>: Send + Sync, 
        D: FlushableDrawTarget + Send + 'static,
        D::Color: RgbColor,
        D::Error: Debug,
    {
        run_draw_engine(
            as_receiver(&self.draw_notif),
            display,
        ).await
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_screen(
    mut button1_command: impl Receiver<Data = ()>,
    mut button2_command: impl Receiver<Data = ()>,
    mut button3_command: impl Receiver<Data = ()>,
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
        let button1_command = button1_command.recv();
        let button2_command = button2_command.recv();
        let button3_command = button3_command.recv();
        let valve = valve.recv();
        let wm = wm.recv();
        let battery = battery.recv();

        pin_mut!(button1_command, button2_command, button3_command, valve, wm, battery);

        let draw_request = match select4(select3(button1_command, button2_command, button3_command), valve, wm, battery).await {
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
