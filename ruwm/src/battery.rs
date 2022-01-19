use core::cell::RefCell;
use core::fmt::{Debug, Display};
use core::time::Duration;

extern crate alloc;
use alloc::rc::Rc;

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::event_bus;
use embedded_svc::timer::{self, Timer};

use crate::storage::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BatteryState {
    voltage: Option<u16>,
    powered: Option<bool>,
}

pub struct Poller<A, BP, PP, P> {
    one_shot: A,
    battery_pin: BP,
    power_pin: PP,
    postbox: P,
}

const EVENT_SOURCE: event_bus::Source<BatteryState> = event_bus::Source::new(b"BATTERY\0");

impl<A, BP, PP, P> Poller<A, BP, PP, P> {
    fn poll<ADC, EE>(&mut self, ss: &mut BatteryState, post: bool) -> anyhow::Result<()>
    where
        A: adc::OneShot<ADC, u16, BP>,
        BP: adc::Channel<ADC>,
        PP: InputPin<Error = EE>,
        EE: Display + Debug + Send + Sync + 'static,
        P: event_bus::Postbox,
    {
        let state = BatteryState {
            voltage: self.one_shot.read(&mut self.battery_pin).ok(),
            powered: Some(self.power_pin.is_high().map_err(|e| anyhow::anyhow!(e))?),
        };

        if *ss != state {
            *ss = state;

            if post {
                self.postbox
                    .post(&EVENT_SOURCE, ss)
                    .map_err(|e| anyhow::anyhow!(e))?;
            }
        }

        Ok(())
    }
}

pub struct Battery<S, T>
where
    T: timer::PinnedTimerService,
{
    _timer: T::Timer,
    storage: S,
    state: Rc<RefCell<BatteryState>>,
}

impl<S, T> Battery<S, T>
where
    S: Storage<BatteryState>,
    T: timer::PinnedTimerService,
{
    pub const EVENT_SOURCE: event_bus::Source<BatteryState> = EVENT_SOURCE;

    pub fn new<ADC, B, EE, A, BP, PP>(
        timer_service: &T,
        postbox: B,
        storage: S,
        one_shot: A,
        battery_pin: BP,
        power_pin: PP,
    ) -> anyhow::Result<Self>
    where
        B: event_bus::Postbox + 'static,
        A: adc::OneShot<ADC, u16, BP> + 'static,
        BP: adc::Channel<ADC> + 'static,
        PP: InputPin<Error = EE> + 'static,
        EE: Display + Debug + Send + Sync + 'static,
    {
        let mut poller = Poller {
            one_shot,
            battery_pin,
            power_pin,
            postbox,
        };

        let mut state = storage.get();

        poller
            .poll(&mut state, false)
            .map_err(|e| anyhow::anyhow!(e))?;

        let state = Rc::new(RefCell::new(state));
        let state_timer = Rc::downgrade(&state);

        let mut timer = timer_service
            .timer(&Default::default(), move || {
                state_timer
                    .upgrade()
                    .map(|s| poller.poll(&mut s.borrow_mut(), true))
                    .unwrap_or(Ok(()))
            })
            .map_err(|e| anyhow::anyhow!(e))?;

        timer
            .periodic(Duration::from_secs(2))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(Battery {
            _timer: timer,
            storage,
            state,
        })
    }

    pub fn state(&self) -> BatteryState {
        *self.state.borrow()
    }
}
