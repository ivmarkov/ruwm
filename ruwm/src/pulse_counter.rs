use core::convert::Infallible;
use core::fmt::Debug;
use core::future::Future;

use embedded_hal::digital::v2::InputPin;

use embassy_time::Duration;

use crate::{
    button::{self, PressedLevel},
    notification::Notification,
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

pub struct CpuPulseCounter<'a, P> {
    pin: P,
    pressed_level: PressedLevel,
    pin_edge: &'a Notification,
    debounce_duration: Option<Duration>,
}

impl<'a, P> CpuPulseCounter<'a, P> {
    pub const fn new(
        pin: P,
        pressed_level: PressedLevel,
        pin_edge: &'a Notification,
        debounce_duration: Option<Duration>,
    ) -> Self {
        Self {
            pin,
            pressed_level,
            pin_edge,
            debounce_duration,
        }
    }
}

impl<'a, P> PulseCounter for CpuPulseCounter<'a, P>
where
    P: InputPin,
{
    type Error = Infallible;

    type TakePulsesFuture<'b> = impl Future<Output = Result<u64, Self::Error>> where Self: 'b;

    fn take_pulses(&mut self) -> Self::TakePulsesFuture<'_> {
        async move {
            button::wait_press(
                &mut self.pin,
                self.pressed_level,
                &mut self.pin_edge,
                self.debounce_duration,
            )
            .await;

            Ok(1)
        }
    }
}

impl<'a, P> PulseWakeup for CpuPulseCounter<'a, P> {
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
