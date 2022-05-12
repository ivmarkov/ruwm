use embedded_svc::utils::asyncs::select::{select3, Either3};
use futures::pin_mut;

use embedded_svc::channel::asyncs::{Receiver, Sender};

use crate::battery::BatteryState;
use crate::error;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::WaterMeterState;

pub async fn run(
    mut notif: impl Sender<Data = ValveCommand>,
    mut valve: impl Receiver<Data = Option<ValveState>>,
    mut wm: impl Receiver<Data = WaterMeterState>,
    mut battery: impl Receiver<Data = BatteryState>,
) -> error::Result<()> {
    let mut valve_state = None;

    loop {
        let valve = valve.recv();
        let wm = wm.recv();
        let battery = battery.recv();

        pin_mut!(valve, wm, battery);

        let emergency_close = match select3(valve, wm, battery).await {
            Either3::First(valve) => {
                let valve = valve.map_err(error::svc)?;

                valve_state = valve;

                false
            }
            Either3::Second(wm) => {
                let wm = wm.map_err(error::svc)?;

                wm.leaking
            }
            Either3::Third(battery) => {
                let battery = battery.map_err(error::svc)?;

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
            notif.send(ValveCommand::Close).await.map_err(error::svc)?;
        }
    }
}
