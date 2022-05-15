use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::signal::asyncs::{SendSyncSignalFamily, Signal};
use embedded_svc::timer::asyncs::OnceTimer;
use embedded_svc::utils::asyncs::select::{select, Either};
use embedded_svc::utils::asyncs::signal::adapt::as_channel;

use crate::error;
use crate::pulse_counter::PulseCounter;
use crate::state_snapshot::StateSnapshot;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WaterMeterState {
    pub edges_count: u64,
    pub armed: bool,
    pub leaking: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum WaterMeterCommand {
    Arm,
    Disarm,
}

pub struct WaterMeter<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    state: StateSnapshot<M::Mutex<WaterMeterState>>,
    command_signal: M::Signal<WaterMeterCommand>,
}

impl<M> WaterMeter<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    pub fn new() -> Self {
        Self {
            state: StateSnapshot::new(),
            command_signal: M::Signal::new(),
        }
    }

    pub fn state(&self) -> &StateSnapshot<impl Mutex<Data = WaterMeterState>> {
        &self.state
    }

    pub fn command_sink(&'static self) -> impl Sender<Data = WaterMeterCommand> + 'static {
        as_channel(&self.command_signal)
    }

    pub async fn process(
        &'static self,
        timer: impl OnceTimer,
        pulse_counter: impl PulseCounter,
        state_sink: impl Sender<Data = WaterMeterState>,
    ) -> error::Result<()> {
        process(
            timer,
            pulse_counter,
            &self.state,
            as_channel(&self.command_signal),
            state_sink,
        )
        .await
    }
}

pub async fn process(
    mut timer: impl OnceTimer,
    mut pulse_counter: impl PulseCounter,
    state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    mut command_source: impl Receiver<Data = WaterMeterCommand>,
    mut state_sink: impl Sender<Data = WaterMeterState>,
) -> error::Result<()> {
    pulse_counter.start().map_err(error::svc)?;

    loop {
        let command = command_source.recv();
        let tick = timer
            .after(Duration::from_secs(2) /*Duration::from_millis(200)*/)
            .map_err(error::svc)?;

        //pin_mut!(command, tick);

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
                        edges_count: state.edges_count + data.edges_count as u64,
                        armed: data.wakeup_edges > 0,
                        leaking: state.edges_count < state.edges_count + data.edges_count as u64
                            && state.armed
                            && data.wakeup_edges > 0,
                    })
                },
                &mut state_sink,
            )
            .await?;
    }
}
