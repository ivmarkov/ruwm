use core::fmt::{Debug, Display};
use core::time::Duration;

use anyhow::anyhow;

use futures::future::{select, Either};
use futures::pin_mut;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::nonblocking::PeriodicTimer;

use crate::pulse_counter::PulseCounter;
use crate::state_snapshot::StateSnapshot;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct WaterMeterState {
    pub prev_edges_count: u64,
    pub prev_armed: bool,
    pub prev_leaking: bool,
    pub edges_count: u64,
    pub armed: bool,
    pub leaking: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum WaterMeterCommand {
    Arm,
    Disarm,
}

pub async fn run<M, C, N, T, PC>(
    state: StateSnapshot<M>,
    mut command: C,
    mut notif: N,
    mut timer: T,
    mut pulse_counter: PC,
) -> anyhow::Result<()>
where
    M: Mutex<Data = WaterMeterState>,
    C: Receiver<Data = WaterMeterCommand>,
    N: Sender<Data = WaterMeterState>,
    T: PeriodicTimer,
    PC: PulseCounter,
    C::Error: Display + Send + Sync + 'static,
    N::Error: Display + Send + Sync + 'static,
    T::Error: Display + Send + Sync + 'static,
{
    pulse_counter.start().map_err(|e| anyhow!(e))?;

    let mut clock = timer
        .every(Duration::from_secs(2) /*Duration::from_millis(200)*/)
        .map_err(|e| anyhow!(e))?;

    loop {
        let command = command.recv();
        let tick = clock.recv();

        pin_mut!(command);
        pin_mut!(tick);

        let data = match select(command, tick).await {
            Either::Left((command, _)) => {
                let command = command.map_err(|e| anyhow!(e))?;

                let mut data = pulse_counter.get_data().map_err(|e| anyhow!(e))?;

                data.edges_count = 0;
                data.wakeup_edges = if command == WaterMeterCommand::Arm {
                    1
                } else {
                    0
                };

                pulse_counter.swap_data(&data).map_err(|e| anyhow!(e))?
            }
            Either::Right(_) => {
                let mut data = pulse_counter.get_data().map_err(|e| anyhow!(e))?;

                data.edges_count = 0;

                pulse_counter.swap_data(&data).map_err(|e| anyhow!(e))?
            }
        };

        state
            .update_with(
                |state| WaterMeterState {
                    prev_edges_count: state.edges_count,
                    prev_armed: state.armed,
                    prev_leaking: state.leaking,
                    edges_count: state.edges_count + data.edges_count as u64,
                    armed: data.wakeup_edges > 0,
                    leaking: state.edges_count < state.edges_count + data.edges_count as u64
                        && state.armed
                        && data.wakeup_edges > 0,
                },
                &mut notif,
            )
            .await?;
    }
}
