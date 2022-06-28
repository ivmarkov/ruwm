use core::fmt::Debug;

use embedded_svc::mutex::RawMutex;
use embedded_svc::utils::asynch::signal::AtomicSignal;
use embedded_svc::utils::mutex::Mutex;
use enumset::{EnumSet, EnumSetType};
use serde::{Deserialize, Serialize};

use embedded_graphics::prelude::RgbColor;

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::unblocker::asynch::Unblocker;
use embedded_svc::utils::asynch::channel::adapt::merge;
use embedded_svc::utils::asynch::select::{select3, select4, Either3, Either4};
use embedded_svc::utils::asynch::signal::adapt::as_channel;

use crate::battery::BatteryState;
use crate::state::StateCellRead;
use crate::utils::{adapt_static_receiver, as_static_receiver, as_static_sender, StaticRef};
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
    state: Mutex<R, ScreenState>,
    button1_pressed_signal: AtomicSignal<()>,
    button2_pressed_signal: AtomicSignal<()>,
    button3_pressed_signal: AtomicSignal<()>,
    valve_state_signal: AtomicSignal<()>,
    wm_state_signal: AtomicSignal<()>,
    wm_stats_state_signal: AtomicSignal<()>,
    battery_state_signal: AtomicSignal<()>,
    draw_request_signal: AtomicSignal<()>,
}

pub struct Q<R>(pub R);

impl<R> Screen<R>
where
    R: RawMutex,
{
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ScreenState {
                changeset: EnumSet::all(),
                ..Default::default()
            }),
            button1_pressed_signal: AtomicSignal::new(),
            button2_pressed_signal: AtomicSignal::new(),
            button3_pressed_signal: AtomicSignal::new(),
            valve_state_signal: AtomicSignal::new(),
            wm_state_signal: AtomicSignal::new(),
            wm_stats_state_signal: AtomicSignal::new(),
            battery_state_signal: AtomicSignal::new(),
            draw_request_signal: AtomicSignal::new(),
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

    pub fn valve_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.valve_state_signal)
    }

    pub fn wm_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.wm_state_signal)
    }

    pub fn wm_stats_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.wm_stats_state_signal)
    }

    pub fn battery_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.battery_state_signal)
    }

    pub async fn draw<D>(&'static self, display: D)
    where
        D: FlushableDrawTarget + Send + 'static,
        D::Color: RgbColor,
        D::Error: Debug,
    {
        run_draw(
            as_static_receiver(&self.draw_request_signal),
            display,
            &self.state,
        )
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
        draw_request_sink: impl Sender<Data = ()> + Send + 'static,
    ) {
        let battery_state_wrapper = StaticRef(battery_state);
        let valve_state_wrapper = StaticRef(valve_state);
        let wm_state_wrapper = StaticRef(wm_state);

        process(
            &self.state,
            as_static_receiver(&self.button1_pressed_signal),
            as_static_receiver(&self.button2_pressed_signal),
            as_static_receiver(&self.button3_pressed_signal),
            adapt_static_receiver(as_static_receiver(&self.valve_state_signal), move |_| {
                Some(valve_state_wrapper.0.get())
            }),
            adapt_static_receiver(as_static_receiver(&self.wm_state_signal), move |_| {
                Some(wm_state_wrapper.0.get())
            }),
            adapt_static_receiver(as_static_receiver(&self.battery_state_signal), move |_| {
                Some(battery_state_wrapper.0.get())
            }),
            merge(
                as_static_sender(&self.draw_request_signal),
                draw_request_sink,
            ),
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn process(
    screen_state: &Mutex<impl RawMutex, ScreenState>,
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
            let mut screen_state = screen_state.lock();

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
        }

        draw_request_sink.send(()).await;
    }
}

pub async fn unblock_run_draw<U, D>(
    unblocker: U,
    mut draw_request: impl Receiver<Data = ()>,
    mut display: D,
    screen_state: &Mutex<impl RawMutex, ScreenState>,
) -> Result<(), D::Error>
where
    U: Unblocker,
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug + Send + 'static,
{
    loop {
        draw_request.recv().await;

        let screen_state = {
            let mut guard = screen_state.lock();

            let screen_state = guard.clone();

            guard.changeset = EnumSet::empty();

            screen_state
        };

        display = unblocker
            .unblock(move || draw(display, screen_state))
            .await?;
    }
}

pub async fn run_draw<D>(
    mut draw_request: impl Receiver<Data = ()>,
    mut display: D,
    screen_state: &Mutex<impl RawMutex, ScreenState>,
) -> Result<(), D::Error>
where
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
    loop {
        draw_request.recv().await;

        let screen_state = {
            let mut guard = screen_state.lock();

            let screen_state = guard.clone();

            guard.changeset = EnumSet::empty();

            screen_state
        };

        display = draw(display, screen_state)?;
    }
}

fn draw<D>(mut display: D, screen_state: ScreenState) -> Result<D, D::Error>
where
    D: FlushableDrawTarget + Send + 'static,
    D::Color: RgbColor,
    D::Error: Debug,
{
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
