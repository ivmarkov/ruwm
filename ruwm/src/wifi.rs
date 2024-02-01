use core::fmt::Debug;

use embassy_futures::select::{select, Either};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use log::info;
use serde::{Deserialize, Serialize};

use embedded_svc::wifi::{asynch::Wifi, AuthMethod, Configuration};

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

pub static COMMAND: Signal<CriticalSectionRawMutex, WifiCommand> = Signal::new();

pub async fn process<W: Wifi>(mut wifi: W) -> Result<(), W::Error> {
    let mut stay_connected = false;

    loop {
        let result = select(COMMAND.wait(), Timer::after(Duration::from_secs(1))).await;

        match result {
            Either::First(command) => {
                if wifi.is_started().await? {
                    let _ = wifi.stop().await?;
                }

                stay_connected = false;

                match command {
                    WifiCommand::SetConfiguration(conf) => {
                        info!("Got configuration: {:?}", conf);

                        wifi.set_configuration(&conf).await?;

                        if !matches!(conf, Configuration::None) {
                            wifi.start().await?;

                            while !wifi.is_started().await? {
                                Timer::after(Duration::from_millis(100)).await;
                            }

                            info!("Wifi started");
                        }

                        match &conf {
                            Configuration::Client(conf) | Configuration::Mixed(conf, _) => {
                                if conf.auth_method != AuthMethod::None {
                                    wifi.connect().await?;

                                    while !wifi.is_connected().await? {
                                        Timer::after(Duration::from_millis(100)).await;
                                    }

                                    stay_connected = true;

                                    info!("Wifi connected");
                                }
                            }
                            _ => (),
                        }
                    }
                }
            }
            Either::Second(_) => {
                if stay_connected {
                    if !wifi.is_connected().await? {
                        info!("Wifi disconnection detected, reconnecting...");

                        wifi.connect().await?;

                        while !wifi.is_connected().await? {
                            Timer::after(Duration::from_millis(100)).await;
                        }

                        info!("Wifi connected");
                    }
                }
            }
        }
    }
}
