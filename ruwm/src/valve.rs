use core::fmt::Debug;
use core::future::*;
use core::time::Duration;

use futures::future::join;
use futures::future::select;
use futures::future::Either;
use futures::pin_mut;

use embedded_svc::channel::nonblocking::Channel;
use embedded_svc::mutex::Mutex;

use embedded_hal::digital::v2::OutputPin;
use embedded_svc::channel::nonblocking::{Receiver, Sender};

use embedded_svc::timer::nonblocking::Once;

use crate::state_snapshot::StateSnapshot;

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

pub async fn run<M, C, N, SC, SN, O, PP, PO, PC>(
    state_snapshot: StateSnapshot<M>,
    command: C,
    notif: N,
    once: O,
    spin_command: SC,
    spin_notif: SN,
    power_pin: PP,
    open_pin: PO,
    close_pin: PC,
) where
    M: Mutex<Data = Option<ValveState>>,
    C: Receiver<Data = ValveCommand>,
    N: Sender<Data = Option<ValveState>>,
    SC: Channel<Data = ValveCommand>,
    SN: Channel<Data = ()>,
    O: Once,
    PP: OutputPin,
    PO: OutputPin,
    PC: OutputPin,
    PP::Error: Debug,
    PO::Error: Debug,
    PC::Error: Debug,
{
    let (spin_command_sender, spin_command_receiver) = spin_command.split();
    let (spin_notif_sender, spin_notif_receiver) = spin_notif.split();

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
                        if !matches!(
                            state_snapshot.get(),
                            Some(ValveState::Open) | Some(ValveState::Opening)
                        ) {
                            spin_command.send(ValveCommand::Open).await.unwrap();
                            Some(ValveState::Opening)
                        } else {
                            None
                        }
                    }
                    ValveCommand::Close => {
                        if !matches!(
                            state_snapshot.get(),
                            Some(ValveState::Closed) | Some(ValveState::Closing)
                        ) {
                            spin_command.send(ValveCommand::Close).await.unwrap();
                            Some(ValveState::Closing)
                        } else {
                            None
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
    T: Once,
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

        let timer: Either<T::AfterFuture, Pending<Result<(), T::Error>>> =
            if current_command.is_some() {
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
