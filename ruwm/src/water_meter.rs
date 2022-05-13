use core::fmt::Debug;
use core::time::Duration;

use futures::pin_mut;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::timer::asyncs::OnceTimer;
use embedded_svc::utils::asyncs::select::{select, Either};
use embedded_svc::utils::asyncs::signal::{MutexSignal, State};
use embedded_svc::utils::asyncs::signal::adapt::{as_sender, as_receiver};

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

pub struct WaterMeter<M> 
where 
    M: MutexFamily,
{
    state: StateSnapshot<M::Mutex<WaterMeterState>>,
    command: MutexSignal<M::Mutex<State<WaterMeterCommand>>, WaterMeterCommand>,
}

impl<M> WaterMeter<M> 
where 
    M: MutexFamily,
{
    pub fn new() -> Self {
        Self {
            state: StateSnapshot::new(),
            command: MutexSignal::new(),
        }
    }

    pub fn state(&self) -> &StateSnapshot<impl Mutex<Data = WaterMeterState>> {
        &self.state
    }
    
    pub fn command(&self) -> impl Sender<Data = WaterMeterCommand> + '_ 
    where 
        M::Mutex<State<WaterMeterCommand>>: Send + Sync, 
    {
        as_sender(&self.command)
    }

    pub async fn run(
        &self, 
        timer: impl OnceTimer,
        pulse_counter: impl PulseCounter,
        state_sender: impl Sender<Data = WaterMeterState>) -> error::Result<()> 
    where 
        M::Mutex<State<WaterMeterCommand>>: Send + Sync, 
    {
        run(
            &self.state,
            as_receiver(&self.command),
            state_sender,
            timer,
            pulse_counter,
        ).await
    }
}

pub async fn run(
    state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
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
