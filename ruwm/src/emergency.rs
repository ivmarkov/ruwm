use embassy_futures::select::{select3, Either3};

use crate::battery::BatteryState;
use crate::channel::{Receiver, Sender};
use crate::notification::Notification;
use crate::state::StateCellRead;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::WaterMeterState;

pub struct Emergency {
    valve_state_notif: Notification,
    wm_state_notif: Notification,
    battery_state_notif: Notification,
}

impl Emergency {
    pub const fn new() -> Self {
        Self {
            valve_state_notif: Notification::new(),
            wm_state_notif: Notification::new(),
            battery_state_notif: Notification::new(),
        }
    }

    pub fn valve_state_sink(&self) -> &Notification {
        &self.valve_state_notif
    }

    pub fn wm_state_sink(&self) -> &Notification {
        &self.wm_state_notif
    }

    pub fn battery_state_sink(&self) -> &Notification {
        &self.battery_state_notif
    }

    pub async fn process(
        &'static self,
        valve_command: impl Sender<Data = ValveCommand>,
        valve_state: &'static (impl StateCellRead<Data = Option<ValveState>> + Send + Sync),
        wm_state: &'static (impl StateCellRead<Data = WaterMeterState> + Send + Sync),
        battery_state: &'static (impl StateCellRead<Data = BatteryState> + Send + Sync),
    ) {
        process(
            (&self.valve_state_notif, valve_state),
            (&self.wm_state_notif, wm_state),
            (&self.battery_state_notif, battery_state),
            valve_command,
        )
        .await
    }
}

pub async fn process(
    mut valve_state_source: impl Receiver<Data = Option<ValveState>>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut battery_state_source: impl Receiver<Data = BatteryState>,
    mut valve_command_sink: impl Sender<Data = ValveCommand>,
) {
    let mut valve_state = None;

    loop {
        let valve = valve_state_source.recv();
        let wm = wm_state_source.recv();
        let battery = battery_state_source.recv();

        //pin_mut!(valve, wm, battery);

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
            valve_command_sink.send(ValveCommand::Close).await;
        }
    }
}
