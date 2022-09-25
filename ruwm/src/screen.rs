use core::cell::RefCell;
use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use log::info;

use enumset::{EnumSet, EnumSetType};

use embassy_futures::select::{select3, select4, Either3, Either4};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;

use embedded_graphics::prelude::RgbColor;

use embedded_svc::executor::asynch::Unblocker;

use crate::battery::BatteryState;
use crate::channel::{LogSender, Receiver, Sender};
use crate::notification::Notification;
use crate::state::StateCellRead;
use crate::valve::ValveState;
use crate::water_meter::WaterMeterState;
use crate::water_meter_stats::WaterMeterStatsState;

pub use adaptors::*;

use self::pages::{Battery, Summary};

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

impl Default for Page {
    fn default() -> Self {
        Self::Summary
    }
}

#[derive(Debug, EnumSetType)]
pub enum DataSource {
    Page,
    Valve,
    WM,
    WMStats,
    Battery,
}

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct ScreenState {
    changeset: EnumSet<DataSource>,
    active_page: Page,
    valve: Option<ValveState>,
    wm: WaterMeterState,
    battery: BatteryState,
}

impl ScreenState {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn valve(&self) -> Option<&Option<ValveState>> {
        self.changeset
            .contains(DataSource::Valve)
            .then(|| &self.valve)
    }

    pub fn wm(&self) -> Option<&WaterMeterState> {
        self.changeset.contains(DataSource::WM).then(|| &self.wm)
    }

    pub fn battery(&self) -> Option<&BatteryState> {
        self.changeset
            .contains(DataSource::Battery)
            .then(|| &self.battery)
    }
}

pub struct Screen<R>
where
    R: RawMutex,
{
    state: Mutex<R, RefCell<ScreenState>>,
    button1_pressed_notif: Notification,
    button2_pressed_notif: Notification,
    button3_pressed_notif: Notification,
    valve_state_notif: Notification,
    wm_state_notif: Notification,
    wm_stats_state_notif: Notification,
    battery_state_notif: Notification,
    draw_request_notif: Notification,
}

pub struct Q<R>(pub R);

impl<R> Screen<R>
where
    R: RawMutex,
{
    pub fn new() -> Self {
        Self {
            state: Mutex::new(RefCell::new(ScreenState::new())),
            button1_pressed_notif: Notification::new(),
            button2_pressed_notif: Notification::new(),
            button3_pressed_notif: Notification::new(),
            valve_state_notif: Notification::new(),
            wm_state_notif: Notification::new(),
            wm_stats_state_notif: Notification::new(),
            battery_state_notif: Notification::new(),
            draw_request_notif: Notification::new(),
        }
    }

    pub fn button1_pressed_sink(&self) -> &Notification {
        &self.button1_pressed_notif
    }

    pub fn button2_pressed_sink(&self) -> &Notification {
        &self.button2_pressed_notif
    }

    pub fn button3_pressed_sink(&self) -> &Notification {
        &self.button3_pressed_notif
    }

    pub fn valve_state_sink(&self) -> &Notification {
        &self.valve_state_notif
    }

    pub fn wm_state_sink(&self) -> &Notification {
        &self.wm_state_notif
    }

    pub fn wm_stats_state_sink(&self) -> &Notification {
        &self.wm_stats_state_notif
    }

    pub fn battery_state_sink(&self) -> &Notification {
        &self.battery_state_notif
    }

    pub async fn draw<D>(&'static self, display: D)
    where
        D: FlushableDrawTarget + Send + 'static,
        D::Color: RgbColor,
        D::Error: Debug,
    {
        run_draw(&self.draw_request_notif, display, &self.state)
            .await
            .unwrap(); // TODO
    }

    pub async fn process(
        &'static self,
        valve_state: &'static (impl StateCellRead<Data = Option<ValveState>> + Send + Sync + 'static),
        wm_state: &'static (impl StateCellRead<Data = WaterMeterState> + Send + Sync + 'static),
        wm_stats_state: &'static (impl StateCellRead<Data = WaterMeterStatsState>
                      + Send
                      + Sync
                      + 'static),
        battery_state: &'static (impl StateCellRead<Data = BatteryState> + Send + Sync + 'static),
    ) {
        process(
            &self.state,
            &self.button1_pressed_notif,
            &self.button2_pressed_notif,
            &self.button3_pressed_notif,
            (&self.valve_state_notif, valve_state),
            (&self.wm_state_notif, wm_state),
            (&self.battery_state_notif, battery_state),
            (LogSender::new("DRAW"), &self.draw_request_notif),
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn process(
    screen_state: &Mutex<impl RawMutex, RefCell<ScreenState>>,
    mut button1_pressed_source: impl Receiver<Data = ()>,
    mut button2_pressed_source: impl Receiver<Data = ()>,
    mut button3_pressed_source: impl Receiver<Data = ()>,
    mut valve_state_source: impl Receiver<Data = Option<ValveState>>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut battery_state_source: impl Receiver<Data = BatteryState>,
    mut draw_request_sink: impl Sender<Data = ()>,
) {
    loop {
        let button1_command = button1_pressed_source.recv();
        let button2_command = button2_pressed_source.recv();
        let button3_command = button3_pressed_source.recv();
        let valve = valve_state_source.recv();
        let wm = wm_state_source.recv();
        let battery = battery_state_source.recv();

        //pin_mut!(button1_command, button2_command, button3_command, valve, wm, battery);

        let sr = select4(
            select3(button1_command, button2_command, button3_command),
            valve,
            wm,
            battery,
        )
        .await;

        {
            screen_state.lock(|screen_state| {
                let mut screen_state = screen_state.borrow_mut();

                match sr {
                    Either4::First(Either3::First(_)) => {
                        screen_state.active_page = screen_state.active_page.prev();
                        screen_state.changeset.insert(DataSource::Page);
                    }
                    Either4::First(Either3::Second(_)) => {
                        screen_state.active_page = screen_state.active_page.next();
                        screen_state.changeset.insert(DataSource::Page);
                    }
                    Either4::First(Either3::Third(_)) => {}
                    Either4::Second(valve) => {
                        screen_state.valve = valve;
                        screen_state.changeset.insert(DataSource::Valve);
                    }
                    Either4::Third(wm) => {
                        screen_state.wm = wm;
                        screen_state.changeset.insert(DataSource::WM);
                    }
                    Either4::Fourth(battery) => {
                        screen_state.battery = battery;
                        screen_state.changeset.insert(DataSource::Battery);
                    }
                }
            });
        }

        draw_request_sink.send(()).await;
    }
}

pub async fn unblock_run_draw<U, D>(
    unblocker: U,
    mut draw_request: impl Receiver<Data = ()>,
    mut display: D,
    screen_state: &Mutex<impl RawMutex, RefCell<ScreenState>>,
) -> Result<(), D::Error>
where
    U: Unblocker,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug + Send + 'static,
{
    loop {
        draw_request.recv().await;

        let screen_state = screen_state.lock(|screen_state| {
            let screen_state_prev = screen_state.borrow().clone();

            screen_state.borrow_mut().changeset = EnumSet::empty();

            screen_state_prev
        });

        display = unblocker
            .unblock(move || draw(display, screen_state))
            .await?;
    }
}

pub async fn run_draw<D>(
    mut draw_request: impl Receiver<Data = ()>,
    mut display: D,
    screen_state: &Mutex<impl RawMutex, RefCell<ScreenState>>,
) -> Result<(), D::Error>
where
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    loop {
        draw_request.recv().await;

        let screen_state = screen_state.lock(|screen_state| {
            let screen_state_prev = screen_state.borrow().clone();

            screen_state.borrow_mut().changeset = EnumSet::empty();

            screen_state_prev
        });

        display = draw(display, screen_state)?;
    }
}

fn draw<D>(mut display: D, screen_state: ScreenState) -> Result<D, D::Error>
where
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    info!("DRAWING: {:?}", screen_state);

    match screen_state.active_page {
        Page::Summary => Summary::draw(
            &mut display,
            screen_state.valve(),
            screen_state.wm(),
            screen_state.battery(),
        )?,
        Page::Battery => Battery::draw(&mut display, screen_state.battery())?,
    }

    display.flush()?;

    Ok(display)
}
