use core::cell::RefCell;
use core::fmt::{Debug, Display};
use core::time::Duration;

extern crate alloc;
use alloc::rc::Rc;

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::event_bus;
use embedded_svc::event_bus::Timer;

use crate::storage::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BatteryState {
    voltage: Option<u16>,
    powered: Option<bool>,
}

pub struct Battery<S, E, T, A, BP, PP> {
    event_bus: E,
    one_shot: A,
    battery_pin: BP,
    power_pin: PP,
    timer: T,
    storage: S,
    state: BatteryState,
}

impl<'a, S, E, A, BP, PP> Battery<S, E, E::Timer<'a>, A, BP, PP>
where
    S: Storage<BatteryState> + 'a,
    E: event_bus::EventBus<'a>,
{
    pub const EVENT_SOURCE: event_bus::Source<BatteryState> = event_bus::Source::new("BATTERY");

    pub fn new<ADC, EE>(
        event_bus: E,
        storage: S,
        one_shot: A,
        battery_pin: BP,
        power_pin: PP,
    ) -> anyhow::Result<Rc<RefCell<Self>>>
    where
        A: adc::OneShot<ADC, u16, BP> + 'a,
        BP: adc::Channel<ADC> + 'a,
        PP: InputPin<Error = EE> + 'a,
        EE: Display + Debug + Send + Sync + 'static,
    {
        let state = Self {
            timer: event_bus
                .timer(Default::default(), Self::EVENT_SOURCE.id())
                .map_err(|e| anyhow::anyhow!(e))?,
            event_bus,
            one_shot,
            battery_pin,
            power_pin,
            state: storage.get(),
            storage,
        };

        let state = Rc::new(RefCell::new(state));
        let weak = Rc::downgrade(&state);

        {
            let timer = &mut state.borrow_mut().timer;

            timer
                .callback(Some(move || {
                    weak.upgrade()
                        .map(|state| state.borrow_mut().poll())
                        .unwrap_or(Ok(()))
                }))
                .map_err(|e| anyhow::anyhow!(e))?;

            timer
                .schedule(Duration::from_secs(0))
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(state)
    }

    pub fn state(&self) -> &BatteryState {
        &self.state
    }

    fn poll<ADC, EE>(&mut self) -> anyhow::Result<()>
    where
        A: adc::OneShot<ADC, u16, BP>,
        BP: adc::Channel<ADC>,
        PP: InputPin<Error = EE>,
        EE: Display + Debug + Send + Sync + 'static,
    {
        let state = BatteryState {
            voltage: self.one_shot.read(&mut self.battery_pin).ok(),
            powered: Some(self.power_pin.is_high().map_err(|e| anyhow::anyhow!(e))?),
        };

        if self.state != state {
            self.state = state;
            self.event_bus
                .post(Default::default(), &Self::EVENT_SOURCE, &self.state);
        }

        self.timer.schedule(Duration::from_secs(2));

        Ok(())
    }
}
