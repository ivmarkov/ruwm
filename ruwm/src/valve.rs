use core::fmt::Debug;
use core::future::pending;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embedded_hal::digital::v2::OutputPin;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::signal::asyncs::{SendSyncSignalFamily, Signal};
use embedded_svc::timer::asyncs::OnceTimer;
use embedded_svc::utils::asyncs::select::{select, Either};
use embedded_svc::utils::asyncs::signal::adapt::{as_receiver, as_sender};

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

pub struct Valve<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    state: StateSnapshot<M::Mutex<Option<ValveState>>>,
    command_signal: M::Signal<ValveCommand>,
    spin_command_signal: M::Signal<ValveCommand>,
    spin_finished_signal: M::Signal<()>,
}

impl<M> Valve<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    pub fn new() -> Self {
        Self {
            state: StateSnapshot::new(),
            command_signal: M::Signal::new(),
            spin_command_signal: M::Signal::new(),
            spin_finished_signal: M::Signal::new(),
        }
    }

    pub fn state(&'static self) -> &'static StateSnapshot<impl Mutex<Data = Option<ValveState>>> {
        &self.state
    }

    pub fn command_sink(&'static self) -> impl Sender<Data = ValveCommand> + 'static {
        as_sender(&self.command_signal)
    }

    pub async fn spin(
        &self,
        once: impl OnceTimer,
        power_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
        open_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
        close_pin: impl OutputPin<Error = impl error::HalError + 'static> + Send + 'static,
    ) -> error::Result<()> {
        spin(
            once,
            power_pin,
            open_pin,
            close_pin,
            as_receiver(&self.spin_command_signal),
            as_sender(&self.spin_finished_signal),
        )
        .await
    }

    pub async fn process(
        &self,
        notif: impl Sender<Data = Option<ValveState>>,
    ) -> error::Result<()> {
        process(
            &self.state,
            as_receiver(&self.command_signal),
            as_receiver(&self.spin_finished_signal),
            as_sender(&self.spin_command_signal),
            notif,
        )
        .await
    }
}

pub async fn spin(
    mut once: impl OnceTimer,
    mut power_pin: impl OutputPin<Error = impl error::HalError>,
    mut open_pin: impl OutputPin<Error = impl error::HalError>,
    mut close_pin: impl OutputPin<Error = impl error::HalError>,
    mut command_source: impl Receiver<Data = ValveCommand>,
    mut spin_finished_sink: impl Sender<Data = ()>,
) -> error::Result<()> {
    let mut current_command: Option<ValveCommand> = None;

    loop {
        start_spin(
            current_command,
            &mut close_pin,
            &mut open_pin,
            &mut power_pin,
        )?;

        let command = command_source.recv();

        let timer = if current_command.is_some() {
            futures::future::Either::Left(once.after(VALVE_TURN_DELAY).map_err(error::svc)?)
        } else {
            futures::future::Either::Right(pending())
        };

        //pin_mut!(command, timer);

        match select(command, timer).await {
            Either::First(command) => {
                current_command = Some(command.map_err(error::svc)?);
            }
            Either::Second(_) => {
                current_command = None;
                spin_finished_sink.send(()).await.map_err(error::svc)?;
            }
        }
    }
}

pub fn start_spin(
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

pub async fn process(
    state: &StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    mut command_source: impl Receiver<Data = ValveCommand>,
    mut spin_finished_source: impl Receiver<Data = ()>,
    mut spin_command_sink: impl Sender<Data = ValveCommand>,
    mut state_sink: impl Sender<Data = Option<ValveState>>,
) -> error::Result<()> {
    loop {
        let current_state = {
            let command = command_source.recv();
            let spin_notif = spin_finished_source.recv();

            //pin_mut!(command, spin_notif);

            match select(command, spin_notif).await {
                Either::First(command) => match command.map_err(error::svc)? {
                    ValveCommand::Open => {
                        let state = state.get();

                        if !matches!(state, Some(ValveState::Open) | Some(ValveState::Opening)) {
                            spin_command_sink
                                .send(ValveCommand::Open)
                                .await
                                .map_err(error::svc)?;
                            Some(ValveState::Opening)
                        } else {
                            state
                        }
                    }
                    ValveCommand::Close => {
                        let state = state.get();

                        if !matches!(state, Some(ValveState::Closed) | Some(ValveState::Closing)) {
                            spin_command_sink
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
                    let state = state.get();

                    match state {
                        Some(ValveState::Opening) => Some(ValveState::Open),
                        Some(ValveState::Closing) => Some(ValveState::Closed),
                        _ => None,
                    }
                }
            }
        };

        state.update(current_state, &mut state_sink).await?;
    }
}
