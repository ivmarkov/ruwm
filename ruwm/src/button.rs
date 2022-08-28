use core::fmt::Debug;
use core::future::pending;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embassy_futures::{select, Either};

use embedded_hal::digital::v2::InputPin;

use embedded_svc::timer::asynch::OnceTimer;

use crate::channel::{Receiver, Sender};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PressedLevel {
    Low,
    High,
}

pub async fn process(
    mut timer: impl OnceTimer,
    mut pin_edge: impl Receiver,
    pin: impl InputPin,
    pressed_level: PressedLevel,
    debounce_time: Option<Duration>,
    mut pressed_sink: impl Sender<Data = ()>,
) {
    let mut debounce = false;

    loop {
        let pin_edge = pin_edge.recv();

        let timer = if debounce {
            futures::future::Either::Left(timer.after(debounce_time.unwrap()).unwrap())
        } else {
            futures::future::Either::Right(pending())
        };

        //pin_mut!(pin_edge, timer);

        let check = match select(pin_edge, timer).await {
            Either::First(_) => {
                if debounce_time.is_some() {
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
                pressed_sink.send(()).await;
            }
        }
    }
}
