use core::fmt::Debug;
use core::future::pending;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::timer::asyncs::OnceTimer;
use embedded_svc::utils::asyncs::select::{select, Either};

use crate::error;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PressedLevel {
    Low,
    High,
}

pub async fn process(
    mut timer: impl OnceTimer,
    mut pin_edge: impl Receiver,
    pin: impl InputPin<Error = impl error::HalError>,
    pressed_level: PressedLevel,
    debounce_time: Option<Duration>,
    mut pressed_sink: impl Sender<Data = ()>,
) -> error::Result<()> {
    let mut debounce = false;

    loop {
        let pin_edge = pin_edge.recv();

        let timer = if debounce {
            futures::future::Either::Left(timer.after(debounce_time.unwrap()).map_err(error::svc)?)
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
            let pressed =
                pin.is_high().map_err(error::hal)? == (pressed_level == PressedLevel::High);

            if pressed {
                pressed_sink.send(()).await.map_err(error::svc)?;
            }
        }
    }
}
