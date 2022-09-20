use core::convert::Infallible;
use core::fmt::Debug;

use futures::Future;

use embedded_hal::digital::v2::InputPin;

use embassy_time::Duration;

use crate::{
    button::{self, PressedLevel},
    channel::Receiver,
};

pub trait PulseCounter {
    type Error: Debug;

    type TakePulsesFuture<'a>: Future<Output = Result<u64, Self::Error>>
    where
        Self: 'a;

    fn take_pulses(&mut self) -> Self::TakePulsesFuture<'_>;
}

impl<T> PulseCounter for &mut T
where
    T: PulseCounter,
{
    type Error = T::Error;

    type TakePulsesFuture<'a> = T::TakePulsesFuture<'a> where Self: 'a;

    fn take_pulses(&mut self) -> Self::TakePulsesFuture<'_> {
        (*self).take_pulses()
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

pub struct CpuPulseCounter<E, P> {
    pin_edge: E,
    pin: P,
    pressed_level: PressedLevel,
    debounce_duration: Option<Duration>,
}

impl<E, P> CpuPulseCounter<E, P> {
    pub const fn new(
        pin_edge: E,
        pin: P,
        pressed_level: PressedLevel,
        debounce_duration: Option<Duration>,
    ) -> Self {
        Self {
            pin_edge,
            pin,
            pressed_level,
            debounce_duration,
        }
    }
}

impl<E, P> PulseCounter for CpuPulseCounter<E, P>
where
    E: Receiver,
    P: InputPin,
{
    type Error = Infallible;

    type TakePulsesFuture<'a> = impl Future<Output = Result<u64, Self::Error>> where Self: 'a;

    fn take_pulses(&mut self) -> Self::TakePulsesFuture<'_> {
        async move {
            button::wait_press(
                &mut self.pin_edge,
                &mut self.pin,
                self.pressed_level,
                self.debounce_duration,
            )
            .await;

            Ok(1)
        }
    }
}

impl<E, P> PulseWakeup for CpuPulseCounter<E, P> {
    type Error = Infallible;

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
