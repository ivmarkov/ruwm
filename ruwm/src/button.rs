use core::fmt::Debug;
use core::future::pending;

use embassy_time::{Duration, Timer};
use serde::{Deserialize, Serialize};

use embassy_futures::select::{select, Either};

use embedded_hal::digital::v2::InputPin;

use crate::channel::{Receiver, Sender};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PressedLevel {
    Low,
    High,
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

        //pin_mut!(pin_edge, timer);

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
