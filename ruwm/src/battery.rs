use core::fmt::Debug;

use embassy_time::{Duration, Timer};

use embedded_hal::digital::InputPin;

use crate::state::State;

pub use crate::dto::battery::*;

pub trait Adc {
    type Error: Debug;

    async fn read(&mut self) -> Result<u16, Self::Error>;
}

pub static STATE: State<BatteryState> = State::new(
    "BATTERY",
    BatteryState::new(),
    &[
        &crate::keepalive::NOTIF,
        &crate::emergency::BATTERY_STATE_NOTIF,
        &crate::screen::BATTERY_STATE_NOTIF,
        &crate::mqtt::BATTERY_STATE_NOTIF,
        &crate::web::BATTERY_STATE_NOTIF,
    ],
);

pub async fn process(mut battery_adc: impl Adc, mut power_pin: impl InputPin) {
    const ROUND_UP: u16 = 50; // TODO: Make it smaller once ADC is connected

    loop {
        Timer::after(Duration::from_secs(2)).await;

        let voltage = battery_adc
            .read()
            .await
            .ok()
            .map(|voltage| voltage / ROUND_UP * ROUND_UP);

        let powered = Some(power_pin.is_high().unwrap_or(false));

        STATE.update(BatteryState { voltage, powered });
    }
}
