use core::fmt::Debug;
use core::time::Duration;

use anyhow::anyhow;

use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::timer::nonblocking::PeriodicTimer;

const POLLING_TIME_MS: u64 = 10;
const DEBOUNCE_TIME_MS: u64 = 50;

pub type ButtonId = u8;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PressedLevel {
    Low,
    High,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ButtonCommand {
    Pressed(ButtonId),
}

pub async fn run<P>(
    id: ButtonId,
    pin: P,
    mut timer: impl PeriodicTimer,
    mut notif: impl Sender<Data = ButtonCommand>,
    pressed_level: PressedLevel,
) -> anyhow::Result<()>
where
    P: InputPin,
    P::Error: Debug,
{
    let mut debounce = 0;

    let mut clock = timer
        .every(Duration::from_millis(POLLING_TIME_MS))
        .map_err(|e| anyhow!(e))?;

    loop {
        clock.recv().await.map_err(|e| anyhow!(e))?;

        let pressed = pin.is_high().unwrap() == (pressed_level == PressedLevel::High);

        if debounce > 0 {
            debounce -= 1;

            if debounce == 0 && pressed {
                notif
                    .send(ButtonCommand::Pressed(id))
                    .await
                    .map_err(|e| anyhow!(e))?;
            }
        } else if pressed {
            debounce = (DEBOUNCE_TIME_MS / POLLING_TIME_MS) as u32;
        }
    }
}
