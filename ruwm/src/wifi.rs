use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use embassy_futures::select::{select, Either};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

use embedded_svc::wifi::{Configuration, Wifi as WifiTrait};

use channel_bridge::asynch::Receiver;

use crate::state::State;

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum WifiCommand {
    SetConfiguration(Configuration),
}

pub static STATE: State<Option<bool>> = State::new(
    "WIFI",
    None,
    &[
        &crate::keepalive::NOTIF,
        &crate::screen::WIFI_STATE_NOTIF,
        &crate::mqtt::WIFI_STATE_NOTIF,
        &crate::web::WIFI_STATE_NOTIF,
    ],
);

pub(crate) static COMMAND: Signal<CriticalSectionRawMutex, WifiCommand> = Signal::new();

pub async fn process<D>(
    mut wifi: impl WifiTrait,
    mut state_changed_source: impl Receiver<Data = D>,
) {
    loop {
        match select(state_changed_source.recv(), COMMAND.wait()).await {
            Either::First(_) => {
                STATE.update(Some(wifi.is_connected().unwrap()));
            }
            Either::Second(command) => match command {
                WifiCommand::SetConfiguration(conf) => wifi.set_configuration(&conf).unwrap(),
            }
        }
    }
}
