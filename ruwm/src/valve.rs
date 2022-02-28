use core::fmt::Debug;
use core::future::pending;
use core::time::Duration;

use futures::future::select;
use futures::future::Either;
use futures::pin_mut;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::nonblocking::OnceTimer;

use embedded_hal::digital::v2::OutputPin;

use crate::error;
use crate::state_snapshot::StateSnapshot;
use crate::storage::Storage;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValveState {
    Open,
    Closed,
    Opening,
    Closing,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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
                Either::Left((command, _)) => match command.map_err(error::svc)? {
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
                Either::Right(_) => {
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
    mut power_pin: impl OutputPin<Error = impl Debug>,
    mut open_pin: impl OutputPin<Error = impl Debug>,
    mut close_pin: impl OutputPin<Error = impl Debug>,
) -> error::Result<()> {
    let mut current_command: Option<ValveCommand> = None;

    loop {
        match current_command {
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

        let command = command.recv();

        let timer = if current_command.is_some() {
            Either::Left(once.after(Duration::from_secs(20)).map_err(error::svc)?)
        } else {
            Either::Right(pending())
        };

        pin_mut!(command, timer);

        match select(command, timer).await {
            Either::Left((command, _)) => {
                current_command = Some(command.map_err(error::svc)?);
            }
            Either::Right(_) => {
                current_command = None;
                complete.send(()).await.map_err(error::svc)?;
            }
        }
    }
}
