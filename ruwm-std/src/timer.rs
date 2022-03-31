use core::convert::Infallible;
use core::future::Future;
use core::time::Duration;

use embedded_svc::channel::asyncs::Receiver;
use embedded_svc::errors::Errors;
use embedded_svc::timer::asyncs::*;

use ruwm::error;

pub fn timers() -> error::Result<impl TimerService> {
    Ok(SmolTimers)
}

struct SmolTimers;
struct SmolTimer;
pub struct SmolInterval(Duration);

impl Errors for SmolTimers {
    type Error = Infallible;
}

impl TimerService for SmolTimers {
    type Timer = SmolTimer;

    fn timer(&mut self) -> Result<Self::Timer, Self::Error> {
        Ok(SmolTimer)
    }
}

impl Errors for SmolTimer {
    type Error = Infallible;
}

impl OnceTimer for SmolTimer {
    type AfterFuture<'a>
    where
        Self: 'a,
    = impl Future<Output = Result<(), Self::Error>>;

    fn after(&mut self, duration: Duration) -> Result<Self::AfterFuture<'_>, Self::Error> {
        Ok(async move {
            smol::Timer::after(duration).await;

            Ok(())
        })
    }
}

impl PeriodicTimer for SmolTimer {
    type Clock<'a>
    where
        Self: 'a,
    = SmolInterval;

    fn every(&mut self, duration: Duration) -> Result<Self::Clock<'_>, Self::Error> {
        Ok(SmolInterval(duration))
    }
}

impl Errors for SmolInterval {
    type Error = Infallible;
}

impl Receiver for SmolInterval {
    type Data = ();

    type RecvFuture<'b>
    where
        Self: 'b,
    = impl Future<Output = Result<Self::Data, Self::Error>>;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move {
            smol::Timer::after(self.0);

            Ok(())
        }
    }
}
