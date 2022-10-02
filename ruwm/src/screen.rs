use core::cell::RefCell;
use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use log::info;

use enumset::{enum_set, EnumSet, EnumSetType};

use embassy_futures::select::{select3, select4, Either3, Either4};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;

use embedded_graphics::prelude::RgbColor;

use embedded_svc::executor::asynch::Unblocker;

use crate::battery::{self, BatteryState};
use crate::notification::Notification;
use crate::valve::{self, ValveState};
use crate::wm::{self, WaterMeterState};

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
    pub const fn new() -> Self {
        Self::Summary
    }

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
        Self::new()
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
    pub const fn new() -> Self {
        Self {
            changeset: enum_set!(
                DataSource::Page
                    | DataSource::Valve
                    | DataSource::WM
                    | DataSource::WMStats
                    | DataSource::Battery
            ),
            active_page: Page::new(),
            valve: None,
            wm: WaterMeterState::new(),
            battery: BatteryState::new(),
        }
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

pub static BUTTON1_PRESSED_NOTIF: Notification = Notification::new();
pub static BUTTON2_PRESSED_NOTIF: Notification = Notification::new();
pub static BUTTON3_PRESSED_NOTIF: Notification = Notification::new();
pub static VALVE_STATE_NOTIF: Notification = Notification::new();
pub static WM_STATE_NOTIF: Notification = Notification::new();
pub static WM_STATS_STATE_NOTIF: Notification = Notification::new();
pub static BATTERY_STATE_NOTIF: Notification = Notification::new();
pub static MQTT_STATE_NOTIF: Notification = Notification::new();
pub static WIFI_STATE_NOTIF: Notification = Notification::new();

static DRAW_REQUEST_NOTIF: Notification = Notification::new();

static STATE: Mutex<CriticalSectionRawMutex, RefCell<ScreenState>> =
    Mutex::new(RefCell::new(ScreenState::new()));

#[allow(clippy::too_many_arguments)]
pub async fn process() {
    loop {
        let sr = select4(
            select3(
                BUTTON1_PRESSED_NOTIF.wait(),
                BUTTON2_PRESSED_NOTIF.wait(),
                BUTTON3_PRESSED_NOTIF.wait(),
            ),
            VALVE_STATE_NOTIF.wait(),
            WM_STATE_NOTIF.wait(),
            BATTERY_STATE_NOTIF.wait(),
        )
        .await;

        {
            STATE.lock(|screen_state| {
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
                    Either4::Second(_) => {
                        screen_state.valve = valve::STATE.get();
                        screen_state.changeset.insert(DataSource::Valve);
                    }
                    Either4::Third(_) => {
                        screen_state.wm = wm::STATE.get();
                        screen_state.changeset.insert(DataSource::WM);
                    }
                    Either4::Fourth(_) => {
                        screen_state.battery = battery::STATE.get();
                        screen_state.changeset.insert(DataSource::Battery);
                    }
                }
            });
        }

        DRAW_REQUEST_NOTIF.notify();
    }
}

pub async fn unblock_run_draw<U, D>(unblocker: U, mut display: D)
where
    U: Unblocker,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug + Send + 'static,
{
    loop {
        DRAW_REQUEST_NOTIF.wait().await;

        let screen_state = STATE.lock(|screen_state| {
            let screen_state_prev = screen_state.borrow().clone();

            screen_state.borrow_mut().changeset = EnumSet::empty();

            screen_state_prev
        });

        display = unblocker
            .unblock(move || draw(display, screen_state))
            .await
            .unwrap();
    }
}

pub async fn run_draw<D>(mut display: D)
where
    D: FlushableDrawTarget,
    D::Color: RgbColor,
    D::Error: Debug,
{
    loop {
        DRAW_REQUEST_NOTIF.wait().await;

        let screen_state = STATE.lock(|screen_state| {
            let screen_state_prev = screen_state.borrow().clone();

            screen_state.borrow_mut().changeset = EnumSet::empty();

            screen_state_prev
        });

        display = draw(display, screen_state).unwrap();
    }
}

fn draw<D>(mut display: D, screen_state: ScreenState) -> Result<D, D::Error>
where
    D: FlushableDrawTarget,
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
