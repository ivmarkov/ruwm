use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use embassy_util::blocking_mutex::raw::RawMutex;
use embassy_util::{select, Either};

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::wifi::{Configuration, Status, Wifi as WifiTrait};

use crate::signal::Signal;
use crate::state::{update, MemoryStateCell, StateCell, StateCellRead};
use crate::utils::SignalReceiver;

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum WifiCommand {
    SetConfiguration(Configuration),
}

pub struct Wifi<R>
where
    R: RawMutex,
{
    state: MemoryStateCell<R, Option<Status>>,
    command: Signal<R, WifiCommand>,
}

impl<R> Wifi<R>
where
    R: RawMutex + Send + Sync + 'static,
{
    pub const fn new() -> Self {
        Self {
            state: MemoryStateCell::new(None),
            command: Signal::new(),
        }
    }

    pub fn state(&self) -> &impl StateCellRead<Data = Option<Status>> {
        &self.state
    }

    pub fn command_sink(&self) -> &Signal<R, WifiCommand> {
        &self.command
    }

    pub async fn process(
        &'static self,
        wifi: impl WifiTrait,
        state_changed_source: impl Receiver<Data = ()>,
        state_sink: impl Sender<Data = ()>,
    ) {
        run(
            wifi,
            &self.state,
            state_changed_source,
            SignalReceiver::new(&self.command),
            state_sink,
        )
        .await
    }
}

pub async fn run(
    mut wifi: impl WifiTrait,
    state: &impl StateCell<Data = Option<Status>>,
    mut state_changed_source: impl Receiver<Data = ()>,
    mut command_source: impl Receiver<Data = WifiCommand>,
    mut state_sink: impl Sender<Data = ()>,
) {
    loop {
        let receiver = state_changed_source.recv();
        let command = command_source.recv();

        //pin_mut!(receiver, command);

        match select(receiver, command).await {
            Either::First(_) => {
                update(state, Some(wifi.get_status()), &mut state_sink).await;
            }
            Either::Second(command) => match command {
                WifiCommand::SetConfiguration(conf) => wifi.set_configuration(&conf).unwrap(),
            },
        }
    }
}
