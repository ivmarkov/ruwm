use core::fmt::Debug;
use core::future::pending;

use embassy_time::{Duration, Timer};
use serde::{Deserialize, Serialize};

use embassy_futures::select::{select, select3, Either, Either3};

use embedded_hal::digital::v2::InputPin;

use channel_bridge::notification::Notification;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PressedLevel {
    Low,
    High,
}

pub static BUTTON1_PIN_EDGE: Notification = Notification::new();
pub static BUTTON2_PIN_EDGE: Notification = Notification::new();
pub static BUTTON3_PIN_EDGE: Notification = Notification::new();

static BUTTON1_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON1_PRESSED_NOTIF];
static BUTTON2_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON2_PRESSED_NOTIF];
static BUTTON3_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON3_PRESSED_NOTIF];

pub async fn button1_process(pin: impl InputPin, pressed_level: PressedLevel) {
    button_process(
        pin,
        pressed_level,
        &BUTTON1_PIN_EDGE,
        "BUTTON1 STATE",
        BUTTON1_NOTIFY,
    )
    .await;
}

pub async fn button2_process(pin: impl InputPin, pressed_level: PressedLevel) {
    button_process(
        pin,
        pressed_level,
        &BUTTON2_PIN_EDGE,
        "BUTTON2 STATE",
        BUTTON2_NOTIFY,
    )
    .await;
}

pub async fn button3_process(pin: impl InputPin, pressed_level: PressedLevel) {
    button_process(
        pin,
        pressed_level,
        &BUTTON3_PIN_EDGE,
        "BUTTON3 STATE",
        BUTTON3_NOTIFY,
    )
    .await;
}

async fn button_process<'a>(
    pin: impl InputPin,
    pressed_level: PressedLevel,
    pin_edge: &'a Notification,
    pressed_sink_msg: &'a str,
    pressed_sink: &'a [&'a Notification],
) {
    process(
        pin,
        pressed_level,
        pin_edge,
        Some(Duration::from_millis(50)),
        pressed_sink_msg,
        pressed_sink,
    )
    .await;
}

pub async fn process<'a>(
    mut pin: impl InputPin,
    pressed_level: PressedLevel,
    pin_edge: &'a Notification,
    debounce_duration: Option<Duration>,
    pressed_sink_msg: &'a str,
    pressed_sink: &'a [&'a Notification],
) {
    loop {
        wait_press(&mut pin, pressed_level, pin_edge, debounce_duration).await;

        log::info!("[{}]", pressed_sink_msg);

        for notification in pressed_sink {
            notification.notify();
        }
    }
}

pub async fn wait_press<'a>(
    pin: &mut impl InputPin,
    pressed_level: PressedLevel,
    pin_edge: &'a Notification,
    debounce_duration: Option<Duration>,
) {
    let mut debounce = false;

    loop {
        let pin_edge = pin_edge.wait();

        let timer = if debounce {
            if let Some(debounce_duration) = debounce_duration {
                futures::future::Either::Left(Timer::after(debounce_duration))
            } else {
                futures::future::Either::Right(pending())
            }
        } else {
            futures::future::Either::Right(pending())
        };

        let check = match select(pin_edge, timer).await {
            Either::First(_) => {
                if debounce_duration.is_some() {
                    debounce = true;
                    false
                } else {
                    true
                }
            }
            Either::Second(_) => {
                if debounce {
                    debounce = false;

                    true
                } else {
                    false
                }
            }
        };

        if check {
            let pressed = pin.is_high().unwrap_or(pressed_level != PressedLevel::High)
                == (pressed_level == PressedLevel::High);

            if pressed {
                return;
            }
        }
    }
}

pub async fn button1_button2_roller_process<'a>(
    pin_1: impl InputPin<Error = impl Debug>,
    pin_2: impl InputPin<Error = impl Debug>,
) {
    roller_process(
        pin_1,
        pin_2,
        &BUTTON1_PIN_EDGE,
        &BUTTON2_PIN_EDGE,
        Some(Duration::from_millis(50)),
        "ROLLER",
        BUTTON1_NOTIFY,
        BUTTON2_NOTIFY,
    )
    .await;
}

pub async fn roller_process<'a>(
    mut pin_1: impl InputPin<Error = impl Debug>,
    mut pin_2: impl InputPin<Error = impl Debug>,
    pin_1_edge: &'a Notification,
    pin_2_edge: &'a Notification,
    debounce_duration: Option<Duration>,
    rolled_sink_msg: &'a str,
    rolled_clockwise_sink: &'a [&'a Notification],
    rolled_counter_clockwise_sink: &'a [&'a Notification],
) {
    loop {
        let clockwise = wait_roller(
            &mut pin_1,
            &mut pin_2,
            pin_1_edge,
            pin_2_edge,
            debounce_duration,
        )
        .await;

        log::info!("[{}]: {}", rolled_sink_msg, clockwise);

        let sink = if clockwise {
            rolled_clockwise_sink
        } else {
            rolled_counter_clockwise_sink
        };

        for notification in sink {
            notification.notify();
        }
    }
}

pub async fn wait_roller<'a>(
    pin_1: &mut impl InputPin<Error = impl Debug>,
    pin_2: &mut impl InputPin<Error = impl Debug>,
    pin_1_edge: &'a Notification,
    pin_2_edge: &'a Notification,
    debounce_duration: Option<Duration>,
) -> bool {
    let mut debounce = false;
    let mut clockwise = false;

    let mut pin_1_was_high = pin_1.is_high().unwrap();
    let mut pin_2_was_high = pin_2.is_high().unwrap();

    loop {
        let pin_a_edge = pin_1_edge.wait();
        let pin_b_edge = pin_2_edge.wait();

        let timer = if debounce {
            if let Some(debounce_duration) = debounce_duration {
                futures::future::Either::Left(Timer::after(debounce_duration))
            } else {
                futures::future::Either::Right(pending())
            }
        } else {
            futures::future::Either::Right(pending())
        };

        let check = match select3(pin_a_edge, pin_b_edge, timer).await {
            Either3::First(_) | Either3::Second(_) => {
                let pin_1_high = pin_1.is_high().unwrap();
                let pin_2_high = pin_2.is_high().unwrap();

                let pin_1_changed = pin_1_high != pin_1_was_high;
                let pin_2_changed = pin_2_high != pin_2_was_high;

                if pin_1_changed != pin_2_changed {
                    clockwise = pin_1_changed;

                    if debounce_duration.is_some() {
                        debounce = true;
                        false
                    } else {
                        return clockwise;
                    }
                } else {
                    pin_1_was_high = pin_1_high;
                    pin_2_was_high = pin_2_high;

                    false
                }
            }
            Either3::Third(_) => {
                if debounce {
                    debounce = false;
                    true
                } else {
                    false
                }
            }
        };

        if check {
            let pin_1_high = pin_1.is_high().unwrap();
            let pin_2_high = pin_2.is_high().unwrap();

            if pin_1_high == pin_2_high && pin_1_high != pin_1_was_high {
                return clockwise;
            }

            pin_1_was_high = pin_1_high;
            pin_2_was_high = pin_2_high;
        }
    }
}
