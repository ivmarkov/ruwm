use core::convert::Infallible;
use core::fmt::Debug;
use core::time::Duration;

use embedded_hal::digital::v2::InputPin;
use embedded_svc::timer::asynch::OnceTimer;
use futures::Future;

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

pub struct CpuPulseCounter<E, P, T> {
    pin_edge: E,
    pin: P,
    pressed_level: PressedLevel,
    debounce_params: Option<(T, Duration)>,
}

impl<E, P, T> CpuPulseCounter<E, P, T> {
    pub const fn new(
        pin_edge: E,
        pin: P,
        pressed_level: PressedLevel,
        debounce_params: Option<(T, Duration)>,
    ) -> Self {
        Self {
            pin_edge,
            pin,
            pressed_level,
            debounce_params,
        }
    }

    pub fn split(&mut self) -> (&mut impl PulseCounter, &mut impl PulseWakeup)
    where
        E: Receiver,
        P: InputPin,
        T: OnceTimer,
    {
        let ptr: *mut Self = self;

        // This is safe because PulseWakeup is a no-op
        unsafe { (ptr.as_mut().unwrap(), ptr.as_mut().unwrap()) }
    }
}

impl<E, P, T> PulseCounter for CpuPulseCounter<E, P, T>
where
    E: Receiver,
    P: InputPin,
    T: OnceTimer,
{
    type Error = Infallible;

    type TakePulsesFuture<'a> = impl Future<Output = Result<u64, Self::Error>> where Self: 'a;

    fn take_pulses(&mut self) -> Self::TakePulsesFuture<'_> {
        async move {
            button::wait_press(
                &mut self.pin_edge,
                &mut self.pin,
                self.pressed_level,
                self.debounce_params
                    .as_mut()
                    .map(|(timer, duration)| (timer, *duration)),
            )
            .await;

            Ok(1)
        }
    }
}

impl<E, P, T> PulseWakeup for CpuPulseCounter<E, P, T> {
    type Error = Infallible;

    fn set_enabled(&mut self, _enabled: bool) -> Result<(), Self::Error> {
        Ok(())
    }
}
