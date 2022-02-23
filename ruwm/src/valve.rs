use core::fmt::{Debug, Display};
use core::future::pending;
use core::time::Duration;

use anyhow::anyhow;

use futures::future::select;
use futures::future::Either;
use futures::pin_mut;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::nonblocking::OnceTimer;

use embedded_hal::digital::v2::OutputPin;
use futures::try_join;

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

#[allow(clippy::too_many_arguments)]
pub async fn run<PP, PO, PC>(
    state_snapshot: StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    command: impl Receiver<Data = ValveCommand>,
    notif: impl Sender<Data = Option<ValveState>>,
    once: impl OnceTimer,
    spin_command_sender: impl Sender<Data = ValveCommand>,
    spin_command_receiver: impl Receiver<Data = ValveCommand>,
    spin_notif_sender: impl Sender<Data = ()>,
    spin_notif_receiver: impl Receiver<Data = ()>,
    power_pin: PP,
    open_pin: PO,
    close_pin: PC,
) -> anyhow::Result<()>
where
    PP: OutputPin,
    PO: OutputPin,
    PC: OutputPin,
    PP::Error: Debug,
    PO::Error: Debug,
    PC::Error: Debug,
{
    try_join! {
        run_events(
            state_snapshot,
            command,
            notif,
            spin_command_sender,
            spin_notif_receiver,
        ),
        run_spin(
            once,
            spin_command_receiver,
            spin_notif_sender,
            power_pin,
            open_pin,
            close_pin,
        ),
    }?;

    Ok(())
}

async fn run_events(
    state_snapshot: StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    mut command: impl Receiver<Data = ValveCommand>,
    mut notif: impl Sender<Data = Option<ValveState>>,
    mut spin_command: impl Sender<Data = ValveCommand>,
    mut spin_notif: impl Receiver<Data = ()>,
) -> anyhow::Result<()> {
    loop {
        let state = {
            let command = command.recv();
            let spin_notif = spin_notif.recv();

            pin_mut!(command);
            pin_mut!(spin_notif);

            match select(command, spin_notif).await {
                Either::Left((command, _)) => match command.map_err(|e| anyhow!(e))? {
                    ValveCommand::Open => {
                        let state = state_snapshot.get();

                        if !matches!(state, Some(ValveState::Open) | Some(ValveState::Opening)) {
                            spin_command
                                .send(ValveCommand::Open)
                                .await
                                .map_err(|e| anyhow!(e))?;
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
                                .map_err(|e| anyhow!(e))?;
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

async fn run_spin<PP, PO, PC>(
    mut once: impl OnceTimer,
    mut command: impl Receiver<Data = ValveCommand>,
    mut complete: impl Sender<Data = ()>,
    mut power_pin: PP,
    mut open_pin: PO,
    mut close_pin: PC,
) -> anyhow::Result<()>
where
    PP: OutputPin,
    PO: OutputPin,
    PC: OutputPin,
    PP::Error: Debug,
    PO::Error: Debug,
    PC::Error: Debug,
{
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
            Either::Left(
                once.after(Duration::from_secs(20))
                    .map_err(|e| anyhow!(e))?,
            )
        } else {
            Either::Right(pending())
        };

        pin_mut!(command);
        pin_mut!(timer);

        match select(command, timer).await {
            Either::Left((command, _)) => {
                current_command = Some(command.map_err(|e| anyhow!(e))?);
            }
            Either::Right(_) => {
                current_command = None;
                complete.send(()).await.map_err(|e| anyhow!(e))?;
            }
        }
    }
}
