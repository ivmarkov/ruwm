use embassy_time::{Duration, Timer};

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use crate::notification::Notification;
use crate::state::State;

pub use crate::dto::battery::*;

pub static STATE_NOTIFY: &[&Notification] = &[
    &crate::keepalive::NOTIF,
    &crate::emergency::BATTERY_STATE_NOTIF,
    &crate::screen::BATTERY_STATE_NOTIF,
    &crate::mqtt::BATTERY_STATE_NOTIF,
];

pub static STATE: State<BatteryState> = State::new(BatteryState::new());

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

        STATE.update_with(
            "BATTERY",
            |_state| BatteryState { voltage, powered },
            STATE_NOTIFY,
        );
    }
}
