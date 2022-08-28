use core::cell::RefCell;
use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embassy_futures::{select, Either};
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};
use embassy_sync::blocking_mutex::Mutex;

use embedded_svc::storage::Storage;
use embedded_svc::timer::asynch::OnceTimer;

use crate::channel::{Receiver, Sender};
use crate::pulse_counter::PulseCounter;
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
                    StorageStateCell::new(storage, "wm"),
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
        timer: impl OnceTimer,
        pulse_counter: impl PulseCounter,
        state_sink: impl Sender<Data = ()>,
    ) {
        process(
            timer,
            pulse_counter,
            &self.state,
            SignalReceiver::new(&self.command_signal),
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
