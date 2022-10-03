use core::fmt::Debug;
use core::future::pending;

use log::info;

use embassy_time::{Duration, Timer};

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

use embedded_hal::blocking::delay::DelayMs;
use embedded_hal::digital::v2::OutputPin;

use crate::notification::Notification;
use crate::state::State;
use crate::web;

pub use crate::dto::valve::*;

pub const TURN_DELAY: Duration = Duration::from_secs(20);

pub static STATE: State<Option<ValveState>, 5, { web::NOTIFY_SIZE }> = State::new(
    "VALVE",
    None,
    [
        &crate::keepalive::NOTIF,
        &crate::emergency::VALVE_STATE_NOTIF,
        &crate::screen::VALVE_STATE_NOTIF,
        &crate::mqtt::VALVE_STATE_NOTIF,
        &STATE_PERSIST_NOTIFY,
    ],
    web::NOTIFY.valve.as_ref(),
);

static STATE_PERSIST_NOTIFY: Notification = Notification::new();

pub(crate) static COMMAND: Signal<CriticalSectionRawMutex, ValveCommand> = Signal::new();

static SPIN_COMMAND: Signal<CriticalSectionRawMutex, ValveCommand> = Signal::new();
static SPIN_FINISHED: Notification = Notification::new();

pub fn emergency_close(
    power_pin: &mut impl OutputPin<Error = impl Debug>,
    open_pin: &mut impl OutputPin<Error = impl Debug>,
    close_pin: &mut impl OutputPin<Error = impl Debug>,
    delay: &mut impl DelayMs<u32>,
) {
    log::error!("Start: emergency closing valve due to ULP wakeup...");

    start_spin(Some(ValveCommand::Close), power_pin, open_pin, close_pin);

    delay.delay_ms((TURN_DELAY.as_secs() * 1000) as u32);

    start_spin(None, power_pin, open_pin, close_pin);

    log::error!("End: emergency closing valve due to ULP wakeup");
}

pub async fn process() {
    loop {
        let current_state = {
            match select(COMMAND.wait(), SPIN_FINISHED.wait()).await {
                Either::First(command) => match command {
                    ValveCommand::Open => {
                        let state = STATE.get();

                        if !matches!(state, Some(ValveState::Open) | Some(ValveState::Opening)) {
                            SPIN_COMMAND.signal(ValveCommand::Open);
                            Some(ValveState::Opening)
                        } else {
                            state
                        }
                    }
                    ValveCommand::Close => {
                        let state = STATE.get();

                        if !matches!(state, Some(ValveState::Closed) | Some(ValveState::Closing)) {
                            SPIN_COMMAND.signal(ValveCommand::Close);
                            Some(ValveState::Closing)
                        } else {
                            state
                        }
                    }
                },
                Either::Second(_) => {
                    let state = STATE.get();

                    match state {
                        Some(ValveState::Opening) => Some(ValveState::Open),
                        Some(ValveState::Closing) => Some(ValveState::Closed),
                        _ => None,
                    }
                }
            }
        };

        STATE.update(current_state);
    }
}

pub async fn spin(
    mut power_pin: impl OutputPin<Error = impl Debug>,
    mut open_pin: impl OutputPin<Error = impl Debug>,
    mut close_pin: impl OutputPin<Error = impl Debug>,
) {
    let mut current_command: Option<ValveCommand> = None;

    loop {
        start_spin(
            current_command,
            &mut power_pin,
            &mut open_pin,
            &mut close_pin,
        );

        let command = COMMAND.wait();

        let timer = if current_command.is_some() {
            futures::future::Either::Left(Timer::after(TURN_DELAY))
        } else {
            futures::future::Either::Right(pending())
        };

        match select(command, timer).await {
            Either::First(command) => {
                current_command = Some(command);
            }
            Either::Second(_) => {
                current_command = None;
                SPIN_FINISHED.notify();
            }
        }
    }
}

fn start_spin(
    command: Option<ValveCommand>,
    power_pin: &mut impl OutputPin<Error = impl Debug>,
    open_pin: &mut impl OutputPin<Error = impl Debug>,
    close_pin: &mut impl OutputPin<Error = impl Debug>,
) {
    info!("============ VALVE COMMAND: {:?}", command);

    match command {
        Some(ValveCommand::Open) => {
            close_pin.set_low().unwrap();
            open_pin.set_high().unwrap();
            power_pin.set_high().unwrap();
        }
        Some(ValveCommand::Close) => {
            open_pin.set_low().unwrap();
            close_pin.set_high().unwrap();
            power_pin.set_high().unwrap();
        }
        None => {
            power_pin.set_low().unwrap();
            open_pin.set_low().unwrap();
            close_pin.set_low().unwrap();
        }
    };
}

pub async fn persist(mut persister: impl FnMut(Option<ValveState>)) {
    loop {
        STATE_PERSIST_NOTIFY.wait().await;

        persister(STATE.get());
    }
}
