use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

use crate::notification::Notification;
use crate::pulse_counter::{PulseCounter, PulseWakeup};
use crate::state::State;

pub use crate::dto::water_meter::*;

pub const FLASH_WRITE_CYCLE: usize = 20;

pub static STATE: State<WaterMeterState, 7> = State::new(
    WaterMeterState::new(),
    [
        &crate::keepalive::NOTIF,
        &crate::emergency::WM_STATE_NOTIF,
        &crate::wm_stats::WM_STATE_NOTIF,
        &crate::screen::WM_STATE_NOTIF,
        &crate::mqtt::WM_STATE_NOTIF,
        &STATE_PERSIST_NOTIFY,
        &STATE_FLASH_NOTIFY,
    ],
);

static STATE_PERSIST_NOTIFY: Notification = Notification::new();
static STATE_FLASH_NOTIFY: Notification = Notification::new();

pub static COMMAND: Signal<CriticalSectionRawMutex, WaterMeterCommand> = Signal::new();

pub async fn process(pulse_counter: impl PulseCounter, pulse_wakeup: impl PulseWakeup) {
    select(
        process_pulses(pulse_counter),
        process_commands(pulse_wakeup),
    )
    .await;
}

async fn process_pulses(mut pulse_counter: impl PulseCounter) {
    loop {
        let pulses = pulse_counter.take_pulses().await.unwrap();

        if pulses > 0 {
            STATE.update_with("WM", |state| WaterMeterState {
                edges_count: state.edges_count + pulses,
                armed: state.armed,
                leaking: state.armed,
            });
        }
    }
}

async fn process_commands(mut pulse_wakeup: impl PulseWakeup) {
    loop {
        let armed = COMMAND.wait().await == WaterMeterCommand::Arm;

        pulse_wakeup.set_enabled(armed).unwrap();

        STATE.update_with("WM", |state| WaterMeterState {
            edges_count: state.edges_count,
            armed,
            leaking: state.leaking,
        });
    }
}

pub async fn persist(mut persister: impl FnMut(WaterMeterState)) {
    loop {
        STATE_PERSIST_NOTIFY.wait().await;

        persister(STATE.get());
    }
}

pub async fn flash(mut flasher: impl FnMut(WaterMeterState)) {
    let mut cycle = 0;

    loop {
        STATE_FLASH_NOTIFY.wait().await;

        cycle += 1;

        if cycle >= FLASH_WRITE_CYCLE {
            cycle = 0;

            flasher(STATE.get());
        }
    }
}
