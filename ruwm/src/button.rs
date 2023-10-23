use core::fmt::Debug;

use embassy_time::{Duration, Timer};
use embedded_hal_async::digital::Wait;
use serde::{Deserialize, Serialize};

use embassy_futures::select::{select, Either};

use embedded_hal::digital::InputPin;

use channel_bridge::notification::Notification;

use crate::log_err;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PressedLevel {
    Low,
    High,
}

static BUTTON1_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON1_PRESSED_NOTIF];
static BUTTON2_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON2_PRESSED_NOTIF];
static BUTTON3_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON3_PRESSED_NOTIF];

pub async fn button1_process(pin: impl InputPin + Wait, pressed_level: PressedLevel) {
    button_process(pin, pressed_level, "BUTTON1 STATE", BUTTON1_NOTIFY).await;
}

pub async fn button2_process(pin: impl InputPin + Wait, pressed_level: PressedLevel) {
    button_process(pin, pressed_level, "BUTTON2 STATE", BUTTON2_NOTIFY).await;
}

pub async fn button3_process(pin: impl InputPin + Wait, pressed_level: PressedLevel) {
    button_process(pin, pressed_level, "BUTTON3 STATE", BUTTON3_NOTIFY).await;
}

async fn button_process<'a>(
    pin: impl InputPin + Wait,
    pressed_level: PressedLevel,
    pressed_sink_msg: &'a str,
    pressed_sink: &'a [&'a Notification],
) {
    process(
        pin,
        pressed_level,
        Some(Duration::from_millis(50)),
        pressed_sink_msg,
        pressed_sink,
    )
    .await;
}

pub async fn process(
    mut pin: impl InputPin + Wait,
    pressed_level: PressedLevel,
    debounce_duration: Option<Duration>,
    pressed_sink_msg: &str,
    pressed_sink: &[&Notification],
) {
    loop {
        log_err!(wait_press(&mut pin, pressed_level, debounce_duration).await);

        log::info!("[{}]", pressed_sink_msg);

        for notification in pressed_sink {
            notification.notify();
        }
    }
}

async fn wait_level<P>(
    pin: &mut P,
    pressed_level: PressedLevel,
    pressed: bool,
    debounce_duration: Option<Duration>,
) -> Result<(), P::Error>
where
    P: InputPin + Wait,
{
    let has_level = |pin: &mut P| {
        if matches!(pressed_level, PressedLevel::Low) && pressed {
            pin.is_low()
        } else {
            pin.is_high()
        }
    };

    loop {
        loop {
            if matches!(pressed_level, PressedLevel::Low) && pressed {
                pin.wait_for_low().await?;
            } else {
                pin.wait_for_high().await?;
            }

            if has_level(pin)? {
                break;
            }
        }

        if let Some(debounce_duration) = debounce_duration {
            if matches!(
                select(pin.wait_for_any_edge(), Timer::after(debounce_duration)).await,
                Either::Second(_)
            ) {
                if has_level(pin)? {
                    break;
                }
            }
        }
    }

    Ok(())
}

pub async fn wait_press<P>(
    pin: &mut P,
    pressed_level: PressedLevel,
    debounce_duration: Option<Duration>,
) -> Result<(), P::Error>
where
    P: InputPin + Wait,
{
    wait_level(pin, pressed_level, false, debounce_duration).await?;
    wait_level(pin, pressed_level, true, debounce_duration).await
}

// pub async fn button1_button2_roller_process<'a>(
//     pin_1: impl InputPin<Error = impl Debug>,
//     pin_2: impl InputPin<Error = impl Debug>,
// ) {
//     roller_process(
//         pin_1,
//         pin_2,
//         Some(Duration::from_millis(50)),
//         "ROLLER",
//         BUTTON1_NOTIFY,
//         BUTTON2_NOTIFY,
//     )
//     .await;
// }

// pub async fn roller_process(
//     mut pin_1: impl InputPin<Error = impl Debug> + Wait,
//     mut pin_2: impl InputPin<Error = impl Debug> + Wait,
//     debounce_duration: Option<Duration>,
//     rolled_sink_msg: &str,
//     rolled_clockwise_sink: &[&Notification],
//     rolled_counter_clockwise_sink: &[&Notification],
// ) {
//     loop {
//         let clockwise = wait_roller(
//             &mut pin_1,
//             &mut pin_2,
//             debounce_duration,
//         )
//         .await;

//         log::info!("[{}]: {}", rolled_sink_msg, clockwise);

//         let sink = if clockwise {
//             rolled_clockwise_sink
//         } else {
//             rolled_counter_clockwise_sink
//         };

//         for notification in sink {
//             notification.notify();
//         }
//     }
// }

// pub async fn wait_roller(
//     pin_1: &mut (impl InputPin<Error = impl Debug> + Wait),
//     pin_2: &mut (impl InputPin<Error = impl Debug> + Wait),
//     debounce_duration: Option<Duration>,
// ) -> bool {
//     let mut debounce = false;
//     let mut clockwise = false;

//     let mut pin_1_was_high = pin_1.is_high().unwrap();
//     let mut pin_2_was_high = pin_2.is_high().unwrap();

//     loop {
//         let pin_a_edge = pin_1_edge.wait();
//         let pin_b_edge = pin_2_edge.wait();

//         let timer = if debounce {
//             if let Some(debounce_duration) = debounce_duration {
//                 futures::future::Either::Left(Timer::after(debounce_duration))
//             } else {
//                 futures::future::Either::Right(pending())
//             }
//         } else {
//             futures::future::Either::Right(pending())
//         };

//         let check = match select3(pin_a_edge, pin_b_edge, timer).await {
//             Either3::First(_) | Either3::Second(_) => {
//                 let pin_1_high = pin_1.is_high().unwrap();
//                 let pin_2_high = pin_2.is_high().unwrap();

//                 let pin_1_changed = pin_1_high != pin_1_was_high;
//                 let pin_2_changed = pin_2_high != pin_2_was_high;

//                 if pin_1_changed != pin_2_changed {
//                     clockwise = pin_1_changed;

//                     if debounce_duration.is_some() {
//                         debounce = true;
//                         false
//                     } else {
//                         return clockwise;
//                     }
//                 } else {
//                     pin_1_was_high = pin_1_high;
//                     pin_2_was_high = pin_2_high;

//                     false
//                 }
//             }
//             Either3::Third(_) => {
//                 if debounce {
//                     debounce = false;
//                     true
//                 } else {
//                     false
//                 }
//             }
//         };

//         if check {
//             let pin_1_high = pin_1.is_high().unwrap();
//             let pin_2_high = pin_2.is_high().unwrap();

//             if pin_1_high == pin_2_high && pin_1_high != pin_1_was_high {
//                 return clockwise;
//             }

//             pin_1_was_high = pin_1_high;
//             pin_2_was_high = pin_2_high;
//         }
//     }
// }
