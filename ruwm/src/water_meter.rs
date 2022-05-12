use core::fmt::Debug;
use core::time::Duration;

use embedded_svc::utils::asyncs::select::{select, Either};
use futures::pin_mut;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::asyncs::OnceTimer;

use crate::error;
use crate::pulse_counter::PulseCounter;
use crate::state_snapshot::StateSnapshot;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WaterMeterState {
    pub prev_edges_count: u64,
    pub prev_armed: bool,
    pub prev_leaking: bool,
    pub edges_count: u64,
    pub armed: bool,
    pub leaking: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum WaterMeterCommand {
    Arm,
    Disarm,
}

pub async fn run(
    state: StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    mut command: impl Receiver<Data = WaterMeterCommand>,
    mut notif: impl Sender<Data = WaterMeterState>,
    mut timer: impl OnceTimer,
    mut pulse_counter: impl PulseCounter,
) -> error::Result<()> {
    pulse_counter.start().map_err(error::svc)?;

    loop {
        let command = command.recv();
        let tick = timer
            .after(Duration::from_secs(2) /*Duration::from_millis(200)*/)
            .map_err(error::svc)?;

        pin_mut!(command, tick);

        let data = match select(command, tick).await {
            Either::First(command) => {
                let command = command.map_err(error::svc)?;

                let mut data = pulse_counter.get_data().map_err(error::svc)?;

                data.edges_count = 0;
                data.wakeup_edges = if command == WaterMeterCommand::Arm {
                    1
                } else {
                    0
                };

                pulse_counter.swap_data(&data).map_err(error::svc)?
            }
            Either::Second(_) => {
                let mut data = pulse_counter.get_data().map_err(error::svc)?;

                data.edges_count = 0;

                pulse_counter.swap_data(&data).map_err(error::svc)?
            }
        };

        state
            .update_with(
                |state| {
                    Ok(WaterMeterState {
                        prev_edges_count: state.edges_count,
                        prev_armed: state.armed,
                        prev_leaking: state.leaking,
                        edges_count: state.edges_count + data.edges_count as u64,
                        armed: data.wakeup_edges > 0,
                        leaking: state.edges_count < state.edges_count + data.edges_count as u64
                            && state.armed
                            && data.wakeup_edges > 0,
                    })
                },
                &mut notif,
            )
            .await?;
    }
}
