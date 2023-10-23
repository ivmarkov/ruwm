use core::fmt::Debug;
use core::future::pending;

use embassy_time::{Duration, Timer};

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

use embedded_hal::delay::DelayUs;
use embedded_hal::digital::OutputPin;

use channel_bridge::notification::Notification;

use crate::state::State;

pub use crate::dto::valve::*;

pub const TURN_TICKS: usize = 20;
pub const TICK_DELAY: Duration = Duration::from_secs(1);

pub static STATE: State<Option<ValveState>> = State::new(
    "VALVE",
    None,
    &[
        &crate::keepalive::NOTIF,
        &crate::emergency::VALVE_STATE_NOTIF,
        &crate::screen::VALVE_STATE_NOTIF,
        &crate::mqtt::VALVE_STATE_NOTIF,
        &crate::web::VALVE_STATE_NOTIF,
        &STATE_PERSIST_NOTIFY,
    ],
);

static STATE_PERSIST_NOTIFY: Notification = Notification::new();

pub(crate) static COMMAND: Signal<CriticalSectionRawMutex, ValveCommand> = Signal::new();

static SPIN_COMMAND: Signal<CriticalSectionRawMutex, ValveCommand> = Signal::new();
static SPIN_WORKING: Signal<CriticalSectionRawMutex, Option<u8>> = Signal::new();

pub fn emergency_close(
    power_pin: &mut impl OutputPin<Error = impl Debug>,
    open_pin: &mut impl OutputPin<Error = impl Debug>,
    close_pin: &mut impl OutputPin<Error = impl Debug>,
    delay: &mut impl DelayUs,
) {
    log::error!("Start: emergency closing valve due to ULP wakeup...");

    start_spin(Some(ValveCommand::Close), power_pin, open_pin, close_pin);

    delay.delay_ms((TICK_DELAY.as_secs() * 1000 * TURN_TICKS as u64) as u32);

    start_spin(None, power_pin, open_pin, close_pin);

    log::error!("End: emergency closing valve due to ULP wakeup");
}

pub async fn process() {
    loop {
        let current_state = {
            match select(COMMAND.wait(), SPIN_WORKING.wait()).await {
                Either::First(command) => match command {
                    ValveCommand::Open => {
                        let state = STATE.get();

                        if !matches!(state, Some(ValveState::Open) | Some(ValveState::Opening(_))) {
                            SPIN_COMMAND.signal(ValveCommand::Open);
                            Some(ValveState::Opening(0))
                        } else {
                            state
                        }
                    }
                    ValveCommand::Close => {
                        let state = STATE.get();

                        if !matches!(
                            state,
                            Some(ValveState::Closed) | Some(ValveState::Closing(_))
                        ) {
                            SPIN_COMMAND.signal(ValveCommand::Close);
                            Some(ValveState::Closing(0))
                        } else {
                            state
                        }
                    }
                },
                Either::Second(progress) => {
                    let state = STATE.get();

                    if let Some(progress) = progress {
                        match state {
                            Some(ValveState::Opening(_)) => Some(ValveState::Opening(progress)),
                            Some(ValveState::Closing(_)) => Some(ValveState::Closing(progress)),
                            _ => None,
                        }
                    } else {
                        match state {
                            Some(ValveState::Opening(_)) => Some(ValveState::Open),
                            Some(ValveState::Closing(_)) => Some(ValveState::Closed),
                            state => state,
                        }
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
    let mut remaining_ticks: usize = 0;

    loop {
        start_spin(
            current_command,
            &mut power_pin,
            &mut open_pin,
            &mut close_pin,
        );

        let command = SPIN_COMMAND.wait();

        let timer = if current_command.is_some() {
            futures::future::Either::Left(Timer::after(TICK_DELAY))
        } else {
            futures::future::Either::Right(pending())
        };

        match select(command, timer).await {
            Either::First(command) => {
                current_command = Some(command);
                remaining_ticks = TURN_TICKS;
            }
            Either::Second(_) => {
                if remaining_ticks > 0 {
                    remaining_ticks -= 1;
                } else {
                    current_command = None;
                }

                SPIN_WORKING.signal(if remaining_ticks > 0 {
                    Some((100 - remaining_ticks * 100 / TURN_TICKS) as u8)
                } else {
                    None
                });
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
