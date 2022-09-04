use core::cell::RefCell;
use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};
use embassy_sync::blocking_mutex::Mutex;

use embedded_svc::storage::Storage;

use crate::channel::{Receiver, Sender};
use crate::pulse_counter::{PulseCounter, PulseWakeup};
use crate::signal::Signal;
use crate::state::{
    update_with, CachingStateCell, MemoryStateCell, MutRefStateCell, StateCell, StateCellRead,
    StorageStateCell,
};
use crate::utils::SignalReceiver;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WaterMeterState {
    pub edges_count: u64,
    pub armed: bool,
    pub leaking: bool,
}

impl WaterMeterState {
    pub const fn new() -> Self {
        Self {
            edges_count: 0,
            armed: false,
            leaking: false,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum WaterMeterCommand {
    Arm,
    Disarm,
}

pub struct WaterMeter<R, S>
where
    R: RawMutex + 'static,
    S: Storage + Send + 'static,
{
    state: CachingStateCell<
        R,
        MemoryStateCell<NoopRawMutex, Option<WaterMeterState>>,
        CachingStateCell<
            NoopRawMutex,
            MutRefStateCell<NoopRawMutex, Option<WaterMeterState>>,
            StorageStateCell<'static, R, S, WaterMeterState>,
        >,
    >,
    command_signal: Signal<R, WaterMeterCommand>,
}

impl<R, S> WaterMeter<R, S>
where
    R: RawMutex + Send + Sync + 'static,
    S: Storage + Send + 'static,
{
    pub fn new(
        state: &'static mut Option<WaterMeterState>,
        storage: &'static Mutex<R, RefCell<S>>,
    ) -> Self {
        Self {
            state: CachingStateCell::new(
                MemoryStateCell::new(None),
                CachingStateCell::new(
                    MutRefStateCell::new(state),
                    StorageStateCell::new(storage, "wm", Default::default()),
                ),
            ),
            command_signal: Signal::new(),
        }
    }

    pub fn state(&self) -> &(impl StateCellRead<Data = WaterMeterState> + Send + Sync) {
        &self.state
    }

    pub fn command_sink(&self) -> &Signal<R, WaterMeterCommand> {
        &self.command_signal
    }

    pub async fn process(
        &'static self,
        pulse_counter: impl PulseCounter,
        pulse_wakeup: impl PulseWakeup,
        state_sink1: impl Sender<Data = ()>,
        state_sink2: impl Sender<Data = ()>,
    ) {
        select(
            process_pulses(pulse_counter, &self.state, state_sink1),
            process_commands(
                pulse_wakeup,
                &self.state,
                SignalReceiver::new(&self.command_signal),
                state_sink2,
            ),
        )
        .await;
    }
}

pub async fn process_pulses(
    mut pulse_counter: impl PulseCounter,
    state: &impl StateCell<Data = WaterMeterState>,
    mut state_sink: impl Sender<Data = ()>,
) {
    loop {
        let pulses = pulse_counter.take_pulses().await.unwrap();

        if pulses > 0 {
            update_with(
                "WM",
                state,
                |state| WaterMeterState {
                    edges_count: state.edges_count + pulses,
                    armed: state.armed,
                    leaking: state.armed,
                },
                &mut state_sink,
            )
            .await;
        }
    }
}

pub async fn process_commands(
    mut pulse_wakeup: impl PulseWakeup,
    state: &impl StateCell<Data = WaterMeterState>,
    mut command_source: impl Receiver<Data = WaterMeterCommand>,
    mut state_sink: impl Sender<Data = ()>,
) {
    loop {
        let armed = command_source.recv().await == WaterMeterCommand::Arm;

        pulse_wakeup.set_enabled(armed).unwrap();

        update_with(
            "WM",
            state,
            |state| WaterMeterState {
                edges_count: state.edges_count,
                armed,
                leaking: state.leaking,
            },
            &mut state_sink,
        )
        .await;
    }
}
