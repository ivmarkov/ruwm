use core::fmt::Debug;
use core::time::Duration;

use embedded_svc::mutex::{NoopRawMutex, RawMutex};
use embedded_svc::utils::asynch::signal::AtomicSignal;
use serde::{Deserialize, Serialize};

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::timer::asynch::OnceTimer;
use embedded_svc::utils::asynch::select::{select, Either};
use embedded_svc::utils::asynch::signal::adapt::as_channel;

use crate::pulse_counter::PulseCounter;
use crate::state::{
    update_with, CachingStateCell, MemoryStateCell, MutRefStateCell, StateCell, StateCellRead,
};

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

pub struct WaterMeter<R>
where
    R: RawMutex,
{
    state: CachingStateCell<
        R,
        MemoryStateCell<NoopRawMutex, Option<WaterMeterState>>,
        MutRefStateCell<NoopRawMutex, WaterMeterState>,
    >,
    command_signal: AtomicSignal<WaterMeterCommand>,
}

impl<R> WaterMeter<R>
where
    R: RawMutex,
{
    pub fn new(state: &'static mut WaterMeterState) -> Self {
        Self {
            state: CachingStateCell::new(MemoryStateCell::new(None), MutRefStateCell::new(state)),
            command_signal: AtomicSignal::new(),
        }
    }

    pub fn state(&self) -> &impl StateCellRead<Data = WaterMeterState> {
        &self.state
    }

    pub fn command_sink(&'static self) -> impl Sender<Data = WaterMeterCommand> + 'static {
        as_channel(&self.command_signal)
    }

    pub async fn process(
        &'static self,
        timer: impl OnceTimer,
        pulse_counter: impl PulseCounter,
        state_sink: impl Sender<Data = ()>,
    ) {
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
    state: &impl StateCell<Data = WaterMeterState>,
    mut command_source: impl Receiver<Data = WaterMeterCommand>,
    mut state_sink: impl Sender<Data = ()>,
) {
    pulse_counter.start().unwrap();

    loop {
        let command = command_source.recv();
        let tick = timer
            .after(Duration::from_secs(2) /*Duration::from_millis(200)*/)
            .unwrap();

        //pin_mut!(command, tick);

        let data = match select(command, tick).await {
            Either::First(command) => {
                let mut data = pulse_counter.get_data().unwrap();

                data.edges_count = 0;
                data.wakeup_edges = if command == WaterMeterCommand::Arm {
                    1
                } else {
                    0
                };

                pulse_counter.swap_data(&data).unwrap()
            }
            Either::Second(_) => {
                let mut data = pulse_counter.get_data().unwrap();

                data.edges_count = 0;

                pulse_counter.swap_data(&data).unwrap()
            }
        };

        update_with(
            state,
            |state| WaterMeterState {
                edges_count: state.edges_count + data.edges_count as u64,
                armed: data.wakeup_edges > 0,
                leaking: state.edges_count < state.edges_count + data.edges_count as u64
                    && state.armed
                    && data.wakeup_edges > 0,
            },
            &mut state_sink,
        )
        .await;
    }
}
