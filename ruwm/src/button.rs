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

pub struct Button<P, T, S> {
    id: ButtonId,
    pin: P,
    timer: T,
    notif: S,
    pressed_level: PressedLevel,
}

impl<P, T, S> Button<P, T, S>
where
    P: InputPin,
    P::Error: Debug,
    T: PeriodicTimer,
    S: Sender<Data = ButtonCommand>,
{
    pub fn new(id: ButtonId, pin: P, timer: T, notif: S, pressed_level: PressedLevel) -> Self {
        Self {
            id,
            pin,
            timer,
            notif,
            pressed_level,
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut debounce = 0;

        let mut clock = self
            .timer
            .every(Duration::from_millis(POLLING_TIME_MS))
            .map_err(|e| anyhow!(e))?;

        loop {
            clock.recv().await.map_err(|e| anyhow!(e))?;

            let pressed = self.pin.is_high().unwrap() == (self.pressed_level == PressedLevel::High);

            if debounce > 0 {
                debounce -= 1;

                if debounce == 0 && pressed {
                    self.notif
                        .send(ButtonCommand::Pressed(self.id))
                        .await
                        .map_err(|e| anyhow!(e))?;
                }
            } else if pressed {
                debounce = (DEBOUNCE_TIME_MS / POLLING_TIME_MS) as u32;
            }
        }
    }
}
