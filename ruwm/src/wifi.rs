use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use embedded_svc::wifi::{asynch::Wifi, Configuration};

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

//pub(crate) static COMMAND: Signal<CriticalSectionRawMutex, WifiCommand> = Signal::new();

pub async fn process(_wifi: impl Wifi) {
    // TODO
    // loop {
    //     match select(state_changed_source.recv(), COMMAND.wait()).await {
    //         Either::First(_) => {
    //             STATE.update(Some(wifi.is_connected().unwrap()));
    //         }
    //         Either::Second(command) => match command {
    //             WifiCommand::SetConfiguration(conf) => wifi.set_configuration(&conf).unwrap(),
    //         },
    //     }
    // }
}
