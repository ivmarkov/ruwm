use core::fmt::Debug;
use core::future::pending;

use log::info;

use embassy_time::{Duration, Timer};

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};
use embassy_sync::signal::Signal;

use embedded_hal::blocking::delay::DelayMs;
use embedded_hal::digital::v2::OutputPin;

use crate::channel::{LogSender, Receiver, Sender};
use crate::notification::Notification;
use crate::state::{
    update, CachingStateCell, MemoryStateCell, MutRefStateCell, StateCell, StateCellRead,
};

pub use crate::dto::valve::*;

pub const VALVE_TURN_DELAY: Duration = Duration::from_secs(20);

pub struct Valve<R>
where
    R: RawMutex + 'static,
{
    state: CachingStateCell<
        R,
        MemoryStateCell<NoopRawMutex, Option<Option<ValveState>>>,
        MutRefStateCell<NoopRawMutex, Option<ValveState>>,
    >,
    command_signal: Signal<R, ValveCommand>,
    spin_command_signal: Signal<R, ValveCommand>,
    spin_finished_notif: Notification,
}

impl<R> Valve<R>
where
    R: RawMutex + Send + Sync + 'static,
{
    pub fn new(state: &'static mut Option<ValveState>) -> Self {
        Self {
            state: CachingStateCell::new(MemoryStateCell::new(None), MutRefStateCell::new(state)),
            command_signal: Signal::new(),
            spin_command_signal: Signal::new(),
            spin_finished_notif: Notification::new(),
        }
    }

    pub fn state(&self) -> &(impl StateCellRead<Data = Option<ValveState>> + Send + Sync) {
        &self.state
    }

    pub fn command_sink(&self) -> &Signal<R, ValveCommand> {
        &self.command_signal
    }

    pub async fn spin(
        &'static self,
        power_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
        open_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
        close_pin: impl OutputPin<Error = impl Debug> + Send + 'static,
    ) {
        spin(
            power_pin,
            open_pin,
            close_pin,
            &self.spin_command_signal,
            (
                LogSender::new("VALVE/SPIN FINISHED"),
                &self.spin_finished_notif,
            ),
        )
        .await
    }

    pub async fn process(&'static self, notif: impl Sender<Data = ()>) {
        process(
            &self.state,
            &self.command_signal,
            &self.spin_finished_notif,
            (
                LogSender::new("VALVE/SPIN COMMAND"),
                &self.spin_command_signal,
            ),
            notif,
        )
        .await
    }
}

pub fn emergency_close(
    power_pin: &mut impl OutputPin<Error = impl Debug>,
    open_pin: &mut impl OutputPin<Error = impl Debug>,
    close_pin: &mut impl OutputPin<Error = impl Debug>,
    delay: &mut impl DelayMs<u32>,
) {
    log::error!("Start: emergency closing valve due to ULP wakeup...");

    start_spin(Some(ValveCommand::Close), power_pin, open_pin, close_pin);

    delay.delay_ms((VALVE_TURN_DELAY.as_secs() * 1000) as u32);

    start_spin(None, power_pin, open_pin, close_pin);

    log::error!("End: emergency closing valve due to ULP wakeup");
}

pub async fn spin(
    mut power_pin: impl OutputPin<Error = impl Debug>,
    mut open_pin: impl OutputPin<Error = impl Debug>,
    mut close_pin: impl OutputPin<Error = impl Debug>,
    mut command_source: impl Receiver<Data = ValveCommand>,
    mut spin_finished_sink: impl Sender<Data = ()>,
) {
    let mut current_command: Option<ValveCommand> = None;

    loop {
        start_spin(
            current_command,
            &mut power_pin,
            &mut open_pin,
            &mut close_pin,
        );

        let command = command_source.recv();

        let timer = if current_command.is_some() {
            futures::future::Either::Left(Timer::after(VALVE_TURN_DELAY))
        } else {
            futures::future::Either::Right(pending())
        };

        //pin_mut!(command, timer);

        match select(command, timer).await {
            Either::First(command) => {
                current_command = Some(command);
            }
            Either::Second(_) => {
                current_command = None;
                spin_finished_sink.send(()).await;
            }
        }
    }
}

pub fn start_spin(
    command: Option<ValveCommand>,
    power_pin: &mut impl OutputPin<Error = impl Debug>,
    open_pin: &mut impl OutputPin<Error = impl Debug>,
    close_pin: &mut impl OutputPin<Error = impl Debug>,
) {
    info!("============ VALVE COMMAND: {:?}", command);

    match command {
        Some(ValveCommand::Open) => {
            close_pin.set_low().unwrap();
            open_pin.set_high().unwrap();
            power_pin.set_high().unwrap();
        }
        Some(ValveCommand::Close) => {
            open_pin.set_low().unwrap();
            close_pin.set_high().unwrap();
            power_pin.set_high().unwrap();
        }
        None => {
            power_pin.set_low().unwrap();
            open_pin.set_low().unwrap();
            close_pin.set_low().unwrap();
        }
    };
}

pub async fn process(
    state: &impl StateCell<Data = Option<ValveState>>,
    mut command_source: impl Receiver<Data = ValveCommand>,
    mut spin_finished_source: impl Receiver<Data = ()>,
    mut spin_command_sink: impl Sender<Data = ValveCommand>,
    mut state_sink: impl Sender<Data = ()>,
) {
    loop {
        let current_state = {
            let command = command_source.recv();
            let spin_notif = spin_finished_source.recv();

            //pin_mut!(command, spin_notif);

            match select(command, spin_notif).await {
                Either::First(command) => match command {
                    ValveCommand::Open => {
                        let state = state.get();

                        if !matches!(state, Some(ValveState::Open) | Some(ValveState::Opening)) {
                            spin_command_sink.send(ValveCommand::Open).await;
                            Some(ValveState::Opening)
                        } else {
                            state
                        }
                    }
                    ValveCommand::Close => {
                        let state = state.get();

                        if !matches!(state, Some(ValveState::Closed) | Some(ValveState::Closing)) {
                            spin_command_sink.send(ValveCommand::Close).await;
                            Some(ValveState::Closing)
                        } else {
                            state
                        }
                    }
                },
                Either::Second(_) => {
                    let state = state.get();

                    match state {
                        Some(ValveState::Opening) => Some(ValveState::Open),
                        Some(ValveState::Closing) => Some(ValveState::Closed),
                        _ => None,
                    }
                }
            }
        };

        update("VALVE", state, current_state, &mut state_sink).await;
    }
}
