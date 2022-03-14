use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::timer::nonblocking::PeriodicTimer;

use crate::error;

const POLLING_TIME_MS: u64 = 10;
const DEBOUNCE_TIME_MS: u64 = 50;

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
    pin: impl InputPin<Error = impl error::HalError>,
    mut timer: impl PeriodicTimer,
    mut notif: impl Sender<Data = ButtonCommand>,
    pressed_level: PressedLevel,
) -> error::Result<()> {
    let mut debounce = 0;

    let mut clock = timer
        .every(Duration::from_millis(POLLING_TIME_MS))
        .map_err(error::svc)?;

    loop {
        clock.recv().await.map_err(error::svc)?;

        let pressed = pin.is_high().map_err(error::hal)? == (pressed_level == PressedLevel::High);

        if debounce > 0 {
            debounce -= 1;

            if debounce == 0 && pressed {
                notif
                    .send(ButtonCommand::Pressed(id))
                    .await
                    .map_err(error::svc)?;
            }
        } else if pressed {
            debounce = (DEBOUNCE_TIME_MS / POLLING_TIME_MS) as u32;
        }
    }
}
