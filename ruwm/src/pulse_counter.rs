use core::convert::Infallible;
use core::fmt::Debug;

use embedded_hal::digital::{ErrorType, InputPin};
use embedded_hal_async::digital::Wait;

use embassy_time::Duration;

use crate::button::{self, PressedLevel};

pub trait PulseCounter {
    type Error: Debug;

    async fn take_pulses(&mut self) -> Result<u64, Self::Error>;
}

impl<T> PulseCounter for &mut T
where
    T: PulseCounter,
{
    type Error = T::Error;

    async fn take_pulses(&mut self) -> Result<u64, Self::Error> {
        (*self).take_pulses().await
    }
}

pub trait PulseWakeup {
    type Error: Debug;

    fn set_enabled(&mut self, enabled: bool) -> Result<(), Self::Error>;
}

impl<T> PulseWakeup for &mut T
where
    T: PulseWakeup,
{
    type Error = T::Error;

    fn set_enabled(&mut self, enabled: bool) -> Result<(), Self::Error> {
        (*self).set_enabled(enabled)
    }
}

pub struct CpuPulseCounter<P> {
    pin: P,
    pressed_level: PressedLevel,
    debounce_duration: Option<Duration>,
}

impl<P> CpuPulseCounter<P> {
    pub const fn new(
        pin: P,
        pressed_level: PressedLevel,
        debounce_duration: Option<Duration>,
    ) -> Self {
        Self {
            pin,
            pressed_level,
            debounce_duration,
        }
    }
}

impl<P> PulseCounter for CpuPulseCounter<P>
where
    P: InputPin + Wait,
{
    type Error = P::Error;

    async fn take_pulses(&mut self) -> Result<u64, Self::Error> {
        button::wait_press(&mut self.pin, self.pressed_level, self.debounce_duration).await?;

        Ok(1)
    }
}

impl<P> PulseWakeup for CpuPulseCounter<P>
where
    P: ErrorType,
{
    type Error = P::Error;

    fn set_enabled(&mut self, _enabled: bool) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl PulseWakeup for () {
    type Error = Infallible;

    fn set_enabled(&mut self, _enabled: bool) -> Result<(), Self::Error> {
        Ok(())
    }
}
