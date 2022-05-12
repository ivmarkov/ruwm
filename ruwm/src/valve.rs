use core::fmt::Debug;
use core::future::pending;
use core::time::Duration;

use embedded_svc::utils::asyncs::select::{select, Either};
use futures::pin_mut;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::asyncs::OnceTimer;

use embedded_hal::digital::v2::OutputPin;

use crate::error;
use crate::state_snapshot::StateSnapshot;
use crate::storage::Storage;

pub const VALVE_TURN_DELAY: Duration = Duration::from_secs(20);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValveState {
    Open,
    Closed,
    Opening,
    Closing,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum ValveCommand {
    Open,
    Close,
}

pub async fn run_events(
    state_snapshot: StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    mut command: impl Receiver<Data = ValveCommand>,
    mut notif: impl Sender<Data = Option<ValveState>>,
    mut spin_command: impl Sender<Data = ValveCommand>,
    mut spin_notif: impl Receiver<Data = ()>,
) -> error::Result<()> {
    loop {
        let state = {
            let command = command.recv();
            let spin_notif = spin_notif.recv();

            pin_mut!(command, spin_notif);

            match select(command, spin_notif).await {
                Either::First(command) => match command.map_err(error::svc)? {
                    ValveCommand::Open => {
                        let state = state_snapshot.get();

                        if !matches!(state, Some(ValveState::Open) | Some(ValveState::Opening)) {
                            spin_command
                                .send(ValveCommand::Open)
                                .await
                                .map_err(error::svc)?;
                            Some(ValveState::Opening)
                        } else {
                            state
                        }
                    }
                    ValveCommand::Close => {
                        let state = state_snapshot.get();

                        if !matches!(state, Some(ValveState::Closed) | Some(ValveState::Closing)) {
                            spin_command
                                .send(ValveCommand::Close)
                                .await
                                .map_err(error::svc)?;
                            Some(ValveState::Closing)
                        } else {
                            state
                        }
                    }
                },
                Either::Second(_) => {
                    let state = state_snapshot.get();

                    match state {
                        Some(ValveState::Opening) => Some(ValveState::Open),
                        Some(ValveState::Closing) => Some(ValveState::Closed),
                        _ => None,
                    }
                }
            }
        };

        state_snapshot.update(state, &mut notif).await?;
    }
}

pub async fn run_spin(
    mut once: impl OnceTimer,
    mut command: impl Receiver<Data = ValveCommand>,
    mut complete: impl Sender<Data = ()>,
    mut power_pin: impl OutputPin<Error = impl error::HalError>,
    mut open_pin: impl OutputPin<Error = impl error::HalError>,
    mut close_pin: impl OutputPin<Error = impl error::HalError>,
) -> error::Result<()> {
    let mut current_command: Option<ValveCommand> = None;

    loop {
        start_run(
            current_command,
            &mut close_pin,
            &mut open_pin,
            &mut power_pin,
        )?;

        let command = command.recv();

        let timer = if current_command.is_some() {
            futures::future::Either::Left(once.after(VALVE_TURN_DELAY).map_err(error::svc)?)
        } else {
            futures::future::Either::Right(pending())
        };

        pin_mut!(command, timer);

        match select(command, timer).await {
            Either::First(command) => {
                current_command = Some(command.map_err(error::svc)?);
            }
            Either::Second(_) => {
                current_command = None;
                complete.send(()).await.map_err(error::svc)?;
            }
        }
    }
}

pub fn start_run(
    command: Option<ValveCommand>,
    power_pin: &mut impl OutputPin<Error = impl error::HalError>,
    open_pin: &mut impl OutputPin<Error = impl error::HalError>,
    close_pin: &mut impl OutputPin<Error = impl error::HalError>,
) -> error::Result<()> {
    match command {
        Some(ValveCommand::Open) => {
            close_pin.set_low().map_err(error::hal)?;
            open_pin.set_high().map_err(error::hal)?;
            power_pin.set_high().map_err(error::hal)?;
        }
        Some(ValveCommand::Close) => {
            open_pin.set_low().map_err(error::hal)?;
            close_pin.set_high().map_err(error::hal)?;
            power_pin.set_high().map_err(error::hal)?;
        }
        None => {
            power_pin.set_low().map_err(error::hal)?;
            open_pin.set_low().map_err(error::hal)?;
            close_pin.set_low().map_err(error::hal)?;
        }
    };

    Ok(())
}
