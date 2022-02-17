use core::fmt::Debug;
use core::future::pending;
use core::time::Duration;

use futures::future::join;
use futures::future::select;
use futures::future::Either;
use futures::pin_mut;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::nonblocking::OnceTimer;

use embedded_hal::digital::v2::OutputPin;

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

pub async fn run<M, C, N, SCS, SCR, SNS, SNR, O, PP, PO, PC>(
    state_snapshot: StateSnapshot<M>,
    command: C,
    notif: N,
    once: O,
    spin_command_sender: SCS,
    spin_command_receiver: SCR,
    spin_notif_sender: SNS,
    spin_notif_receiver: SNR,
    power_pin: PP,
    open_pin: PO,
    close_pin: PC,
) where
    M: Mutex<Data = Option<ValveState>>,
    C: Receiver<Data = ValveCommand>,
    N: Sender<Data = Option<ValveState>>,
    SCS: Sender<Data = ValveCommand>,
    SCR: Receiver<Data = ValveCommand>,
    SNS: Sender<Data = ()>,
    SNR: Receiver<Data = ()>,
    O: OnceTimer,
    PP: OutputPin,
    PO: OutputPin,
    PC: OutputPin,
    PP::Error: Debug,
    PO::Error: Debug,
    PC::Error: Debug,
{
    join(
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
    )
    .await;
}

async fn run_events<M, C, N, SC, SN>(
    state_snapshot: StateSnapshot<M>,
    mut command: C,
    mut notif: N,
    mut spin_command: SC,
    mut spin_notif: SN,
) where
    M: Mutex<Data = Option<ValveState>>,
    C: Receiver<Data = ValveCommand>,
    N: Sender<Data = Option<ValveState>>,
    SC: Sender<Data = ValveCommand>,
    SN: Receiver<Data = ()>,
{
    loop {
        let state = {
            let command = command.recv();
            let spin_notif = spin_notif.recv();

            pin_mut!(command);
            pin_mut!(spin_notif);

            match select(command, spin_notif).await {
                Either::Left((command, _)) => match command.unwrap() {
                    ValveCommand::Open => {
                        let state = state_snapshot.get();

                        if !matches!(state, Some(ValveState::Open) | Some(ValveState::Opening)) {
                            spin_command.send(ValveCommand::Open).await.unwrap();
                            Some(ValveState::Opening)
                        } else {
                            state
                        }
                    }
                    ValveCommand::Close => {
                        let state = state_snapshot.get();

                        if !matches!(state, Some(ValveState::Closed) | Some(ValveState::Closing)) {
                            spin_command.send(ValveCommand::Close).await.unwrap();
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

        state_snapshot.update(state, &mut notif).await;
    }
}

async fn run_spin<T, R, C, PP, PO, PC>(
    mut once: T,
    mut command: R,
    mut complete: C,
    mut power_pin: PP,
    mut open_pin: PO,
    mut close_pin: PC,
) where
    T: OnceTimer,
    R: Receiver<Data = ValveCommand>,
    C: Sender<Data = ()>,
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
            Either::Left(once.after(Duration::from_secs(20)).unwrap())
        } else {
            Either::Right(pending())
        };

        pin_mut!(command);
        pin_mut!(timer);

        match select(command, timer).await {
            Either::Left((command, _)) => {
                current_command = Some(command.unwrap());
            }
            Either::Right(_) => {
                current_command = None;
                complete.send(()).await.unwrap();
            }
        }
    }
}
