use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::errors::wrap::WrapError;
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::signal::asynch::{SendSyncSignalFamily, Signal};
use embedded_svc::utils::asynch::select::{select, Either};
use embedded_svc::utils::asynch::signal::adapt::as_channel;
use embedded_svc::wifi::{Configuration, Status, Wifi as WifiTrait};

use crate::state_snapshot::StateSnapshot;
use crate::utils::as_static_receiver;

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum WifiCommand {
    SetConfiguration(Configuration),
}

pub struct Wifi<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    state: StateSnapshot<M::Mutex<Option<Status>>>,
    command: M::Signal<WifiCommand>,
}

impl<M> Wifi<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    pub fn new() -> Self {
        Self {
            state: StateSnapshot::new(),
            command: M::Signal::new(),
        }
    }

    pub fn state(&self) -> &StateSnapshot<impl Mutex<Data = Option<Status>>> {
        &self.state
    }

    pub fn command_sink(&'static self) -> impl Sender<Data = WifiCommand> + '_ {
        as_channel(&self.command)
    }

    pub async fn process(
        &'static self,
        wifi: impl WifiTrait,
        state_changed_source: impl Receiver<Data = ()>,
        state_sink: impl Sender<Data = Option<Status>>,
    ) {
        run(
            wifi,
            &self.state,
            state_changed_source,
            as_static_receiver(&self.command),
            state_sink,
        )
        .await
    }
}

pub async fn run(
    mut wifi: impl WifiTrait,
    state: &StateSnapshot<impl Mutex<Data = Option<Status>>>,
    mut state_changed_source: impl Receiver<Data = ()>,
    mut command_source: impl Receiver<Data = WifiCommand>,
    mut state_sink: impl Sender<Data = Option<Status>>,
) {
    loop {
        let receiver = state_changed_source.recv();
        let command = command_source.recv();

        //pin_mut!(receiver, command);

        match select(receiver, command).await {
            Either::First(_) => {
                state.update(Some(wifi.get_status()), &mut state_sink).await;
            }
            Either::Second(command) => match command {
                WifiCommand::SetConfiguration(conf) => wifi.set_configuration(&conf).unwrap(),
            },
        }
    }
}
