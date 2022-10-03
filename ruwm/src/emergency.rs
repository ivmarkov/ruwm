use embassy_futures::select::{select3, Either3};

use crate::battery::{self, BatteryState};
use crate::notification::Notification;
use crate::valve::{self, ValveCommand, ValveState};
use crate::wm;

pub(crate) static VALVE_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WM_STATE_NOTIF: Notification = Notification::new();
pub(crate) static BATTERY_STATE_NOTIF: Notification = Notification::new();

pub async fn process() {
    let mut valve_state = None;

    loop {
        let emergency_close = match select3(
            VALVE_STATE_NOTIF.wait(),
            WM_STATE_NOTIF.wait(),
            BATTERY_STATE_NOTIF.wait(),
        )
        .await
        {
            Either3::First(_) => {
                valve_state = valve::STATE.get();

                false
            }
            Either3::Second(_) => wm::STATE.get().leaking,
            Either3::Third(_) => {
                let battery = battery::STATE.get();

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
