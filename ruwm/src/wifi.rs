use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

use embedded_svc::wifi::{Configuration, Wifi as WifiTrait};

use crate::channel::Receiver;
use crate::notification::Notification;
use crate::state::State;

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum WifiCommand {
    SetConfiguration(Configuration),
}

pub static STATE_NOTIFY: &[&Notification] = &[
    &crate::keepalive::NOTIF,
    &crate::screen::WIFI_STATE_NOTIF,
    &crate::mqtt::WIFI_STATE_NOTIF,
];

pub static STATE: State<Option<bool>> = State::new(None);

pub static COMMAND: Signal<CriticalSectionRawMutex, WifiCommand> = Signal::new();

pub async fn process<E>(
    mut wifi: impl WifiTrait,
    mut state_changed_source: impl Receiver<Data = E>,
) {
    loop {
        let receiver = state_changed_source.recv();
        let command = COMMAND.wait();

        match select(receiver, command).await {
            Either::First(_) => {
                STATE
                    .update("WIFI", Some(wifi.is_connected().unwrap()), STATE_NOTIFY)
                    .await;
            }
            Either::Second(command) => match command {
                WifiCommand::SetConfiguration(conf) => wifi.set_configuration(&conf).unwrap(),
            },
        }
    }
}
