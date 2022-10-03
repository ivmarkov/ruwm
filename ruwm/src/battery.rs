use embassy_time::{Duration, Timer};

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use crate::state::State;
use crate::web;

pub use crate::dto::battery::*;

pub static STATE: State<BatteryState, 4, { web::NOTIFY_SIZE }> = State::new(
    "BATTERY",
    BatteryState::new(),
    [
        &crate::keepalive::NOTIF,
        &crate::emergency::BATTERY_STATE_NOTIF,
        &crate::screen::BATTERY_STATE_NOTIF,
        &crate::mqtt::BATTERY_STATE_NOTIF,
    ],
    web::NOTIFY.battery.as_ref(),
);

pub async fn process<ADC, BP>(
    mut one_shot: impl adc::OneShot<ADC, u16, BP>,
    mut battery_pin: BP,
    power_pin: impl InputPin,
) where
    BP: adc::Channel<ADC>,
{
    const ROUND_UP: u16 = 50; // TODO: Make it smaller once ADC is connected

    loop {
        Timer::after(Duration::from_secs(2)).await;

        let voltage = Some(100);
        // let voltage = one_shot
        //     .read(&mut battery_pin)
        //     .ok()
        //     .map(|voltage| voltage / ROUND_UP * ROUND_UP);

        let powered = Some(power_pin.is_high().unwrap_or(false));

        STATE.update(BatteryState { voltage, powered });
    }
}
