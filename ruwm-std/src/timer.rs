use core::convert::Infallible;
use core::future::Future;
use core::time::Duration;

use embedded_svc::timer::asynch::*;

pub fn timers() -> impl TimerService {
    SmolTimers
}

struct SmolTimers;
struct SmolTimer;
pub struct SmolInterval(Duration);

impl ErrorType for SmolTimers {
    type Error = Infallible;
}

impl TimerService for SmolTimers {
    type Timer = SmolTimer;

    fn timer(&mut self) -> Result<Self::Timer, Self::Error> {
        Ok(SmolTimer)
    }
}

impl ErrorType for SmolTimer {
    type Error = Infallible;
}

impl OnceTimer for SmolTimer {
    type AfterFuture<'a> = impl Future<Output = ()> + Send
    where Self: 'a;

    fn after(&mut self, duration: Duration) -> Result<Self::AfterFuture<'_>, Self::Error> {
        let fut = async move {
            smol::Timer::after(duration).await;
        };

        Ok(fut)
    }
}

impl PeriodicTimer for SmolTimer {
    type Clock<'a> = SmolInterval where Self: 'a;

    fn every(&mut self, duration: Duration) -> Result<Self::Clock<'_>, Self::Error> {
        Ok(SmolInterval(duration))
    }
}

impl ErrorType for SmolInterval {
    type Error = Infallible;
}

impl Clock for SmolInterval {
    type TickFuture<'b>
    = impl Future<Output = ()> + Send
    where Self: 'b;

    fn tick(&mut self) -> Self::TickFuture<'_> {
        async move {
            smol::Timer::after(self.0);
        }
    }
}
