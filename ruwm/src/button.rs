use core::fmt::Debug;
use core::future::pending;
use core::time::Duration;

use embedded_svc::utils::asyncs::select::{select, Either};
use futures::pin_mut;

use serde::{Deserialize, Serialize};

use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::timer::asyncs::OnceTimer;

use crate::error;

pub type ButtonId = u8;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PressedLevel {
    Low,
    High,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ButtonCommand {
    Pressed(ButtonId),
}

pub async fn run(
    id: ButtonId,
    mut pin_edge: impl Receiver,
    pin: impl InputPin<Error = impl error::HalError>,
    mut timer: impl OnceTimer,
    mut notif: impl Sender<Data = ButtonCommand>,
    pressed_level: PressedLevel,
    debounce_time: Option<Duration>,
) -> error::Result<()> {
    let mut debounce = false;

    loop {
        let pin_edge = pin_edge.recv();

        let timer = if debounce {
            futures::future::Either::Left(timer.after(debounce_time.unwrap()).map_err(error::svc)?)
        } else {
            futures::future::Either::Right(pending())
        };

        pin_mut!(pin_edge, timer);

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
                notif
                    .send(ButtonCommand::Pressed(id))
                    .await
                    .map_err(error::svc)?;
            }
        }
    }
}
