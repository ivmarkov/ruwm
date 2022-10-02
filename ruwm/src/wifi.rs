use core::fmt::Debug;
use core::future::Future;

use serde::{Deserialize, Serialize};

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

use embedded_svc::wifi::{Configuration, Wifi as WifiTrait};

use crate::state::State;

pub trait WifiNotification {
    type WaitFuture<'a>: Future<Output = ()>
    where
        Self: 'a;

    fn wait(&mut self) -> Self::WaitFuture<'_>;
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum WifiCommand {
    SetConfiguration(Configuration),
}

pub static STATE: State<Option<bool>, 3> = State::new(
    "WIFI",
    None,
    [
        &crate::keepalive::NOTIF,
        &crate::screen::WIFI_STATE_NOTIF,
        &crate::mqtt::WIFI_STATE_NOTIF,
    ],
);

pub static COMMAND: Signal<CriticalSectionRawMutex, WifiCommand> = Signal::new();

pub async fn process(mut wifi: impl WifiTrait, mut state_changed_source: impl WifiNotification) {
    loop {
        match select(state_changed_source.wait(), COMMAND.wait()).await {
            Either::First(_) => {
                STATE.update(Some(wifi.is_connected().unwrap()));
            }
            Either::Second(command) => match command {
                WifiCommand::SetConfiguration(conf) => wifi.set_configuration(&conf).unwrap(),
            },
        }
    }
}
