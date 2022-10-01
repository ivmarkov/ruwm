use embassy_futures::select::{select3, Either3};

use crate::battery::{self, BatteryState};
use crate::channel::Receiver;
use crate::notification::Notification;
use crate::valve::{self, ValveCommand, ValveState};
use crate::wm;

pub static VALVE_STATE_NOTIF: Notification = Notification::new();
pub static WM_STATE_NOTIF: Notification = Notification::new();
pub static BATTERY_STATE_NOTIF: Notification = Notification::new();

pub async fn process() {
    let mut valve_source = (&VALVE_STATE_NOTIF, &valve::STATE);
    let mut wm_source = (&WM_STATE_NOTIF, &wm::STATE);
    let mut battery_source = (&BATTERY_STATE_NOTIF, &battery::STATE);

    let mut valve_state = None;

    loop {
        let valve = valve_source.recv();
        let wm = wm_source.recv();
        let battery = battery_source.recv();

        let emergency_close = match select3(valve, wm, battery).await {
            Either3::First(valve) => {
                valve_state = valve;

                false
            }
            Either3::Second(wm) => wm.leaking,
            Either3::Third(battery) => {
                let battery_low = battery
                    .voltage
                    .map(|voltage| voltage <= BatteryState::LOW_VOLTAGE)
                    .unwrap_or(false);

                let powered = battery.powered.unwrap_or(false);

                battery_low && !powered
            }
        };

        if emergency_close
            && !matches!(
                valve_state,
                Some(ValveState::Closing) | Some(ValveState::Closed)
            )
        {
            valve::COMMAND.signal(ValveCommand::Close);
        }
    }
}
