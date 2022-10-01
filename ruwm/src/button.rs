use core::fmt::Debug;
use core::future::pending;

use embassy_time::{Duration, Timer};
use serde::{Deserialize, Serialize};

use embassy_futures::select::{select, Either};

use embedded_hal::digital::v2::InputPin;

use crate::{
    channel::{LogSender, Receiver, Sender},
    notification::Notification,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PressedLevel {
    Low,
    High,
}

pub static BUTTON1_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON1_PRESSED_NOTIF];
pub static BUTTON2_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON2_PRESSED_NOTIF];
pub static BUTTON3_NOTIFY: &[&Notification] = &[&crate::screen::BUTTON3_PRESSED_NOTIF];

pub static BUTTON1_PIN_EDGE: Notification = Notification::new();
pub static BUTTON2_PIN_EDGE: Notification = Notification::new();
pub static BUTTON3_PIN_EDGE: Notification = Notification::new();

pub async fn button1_process(pin: impl InputPin, pressed_level: PressedLevel) {
    button_process(
        pin,
        pressed_level,
        &BUTTON1_PIN_EDGE,
        (BUTTON1_NOTIFY, LogSender::new("BUTTON1 STATE")),
    )
    .await;
}

pub async fn button2_process(pin: impl InputPin, pressed_level: PressedLevel) {
    button_process(
        pin,
        pressed_level,
        &BUTTON2_PIN_EDGE,
        (BUTTON2_NOTIFY, LogSender::new("BUTTON2 STATE")),
    )
    .await;
}

pub async fn button3_process(pin: impl InputPin, pressed_level: PressedLevel) {
    button_process(
        pin,
        pressed_level,
        &BUTTON1_PIN_EDGE,
        (BUTTON3_NOTIFY, LogSender::new("BUTTON3 STATE")),
    )
    .await;
}

async fn button_process(
    pin: impl InputPin,
    pressed_level: PressedLevel,
    pin_edge: &Notification,
    pressed_sink: impl Sender<Data = ()>,
) {
    process(
        pin_edge,
        pin,
        pressed_level,
        Some(Duration::from_millis(50)),
        pressed_sink,
    )
    .await
}

pub async fn process(
    mut pin_edge: impl Receiver,
    mut pin: impl InputPin,
    pressed_level: PressedLevel,
    debounce_duration: Option<Duration>,
    mut pressed_sink: impl Sender<Data = ()>,
) {
    loop {
        wait_press(&mut pin_edge, &mut pin, pressed_level, debounce_duration).await;

        pressed_sink.send(()).await;
    }
}

pub async fn wait_press(
    mut pin_edge: impl Receiver,
    pin: &mut impl InputPin,
    pressed_level: PressedLevel,
    debounce_duration: Option<Duration>,
) {
    let mut debounce = false;

    loop {
        let pin_edge = pin_edge.recv();

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
