use core::fmt::Display;

use anyhow::anyhow;

use futures::future::{select, Either};
use futures::pin_mut;

use embedded_svc::channel::nonblocking::{Receiver, Sender};

use crate::battery::BatteryState;
use crate::valve::ValveCommand;
use crate::water_meter::WaterMeterState;

pub async fn run<N, W, B>(
    mut notif: N,
    mut wm_status: W,
    mut battery_status: B,
) -> anyhow::Result<()>
where
    N: Sender<Data = ValveCommand>,
    W: Receiver<Data = WaterMeterState>,
    B: Receiver<Data = BatteryState>,
    N::Error: Display + Send + Sync + 'static,
    W::Error: Display + Send + Sync + 'static,
    B::Error: Display + Send + Sync + 'static,
{
    loop {
        let wm = wm_status.recv();
        let battery = battery_status.recv();

        pin_mut!(wm);
        pin_mut!(battery);

        let emergency_close = match select(wm, battery).await {
            Either::Left((wm_state, _)) => {
                let wm_state = wm_state.map_err(|e| anyhow!(e))?;

                wm_state.leaking
            }
            Either::Right((battery_state, _)) => {
                let battery_state = battery_state.map_err(|e| anyhow!(e))?;

                battery_state
                    .voltage
                    .map(|voltage| voltage <= BatteryState::LOW_VOLTAGE)
                    .unwrap_or(false)
            }
        };

        if emergency_close {
            notif
                .send(ValveCommand::Close)
                .await
                .map_err(|e| anyhow!(e))?;
        }
    }
}
