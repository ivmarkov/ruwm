use core::cell::RefCell;
use core::time::Duration;

extern crate alloc;
use alloc::rc::Rc;

use embedded_svc::event_bus;
use embedded_svc::event_bus::Timer;

use crate::storage::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValveState {
    Open,
    Closed,
    Opening,
    Closing,
}

pub struct Valve<S, E, T> {
    event_bus: E,
    timer: T,
    storage: S,
    state: Option<ValveState>,
}

impl<'a, S, E> Valve<S, E, E::Timer<'a>>
where
    S: Storage<Option<ValveState>> + 'a,
    E: event_bus::EventBus<'a>,
{
    pub const EVENT_SOURCE: event_bus::Source<Option<ValveState>> = event_bus::Source::new("VALVE");

    pub fn new(event_bus: E, storage: S) -> anyhow::Result<Rc<RefCell<Self>>> {
        let state = Self {
            timer: event_bus
                .timer(event_bus::Priority::VeryHigh, Self::EVENT_SOURCE.id())
                .map_err(|e| anyhow::anyhow!(e))?,
            event_bus,
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
                        .map(|state| state.borrow_mut().complete())
                        .unwrap_or(Ok(()))
                }))
                .map_err(|e| anyhow::anyhow!(e))?;

            timer
                .schedule(Duration::from_secs(0))
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(state)
    }

    pub fn state(&self) -> Option<ValveState> {
        self.state
    }

    pub fn open(&mut self, force: bool) -> anyhow::Result<()> {
        if self.state != Some(ValveState::Open) || force {
            self.set_state(Some(ValveState::Opening))?;

            self.timer
                .schedule(Duration::from_secs(20))
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(())
    }

    pub fn close(&mut self, force: bool) -> anyhow::Result<()> {
        if self.state != Some(ValveState::Closed) || force {
            self.set_state(Some(ValveState::Closing))?;

            self.timer
                .schedule(Duration::from_secs(20))
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(())
    }

    fn complete(&mut self) -> anyhow::Result<()> {
        let state = match self.state {
            Some(ValveState::Opening) => Some(ValveState::Open),
            Some(ValveState::Closing) => Some(ValveState::Closed),
            other => other,
        };

        self.set_state(state)
    }

    fn set_state(&mut self, state: Option<ValveState>) -> anyhow::Result<()> {
        if self.state != state {
            self.state = state;

            self.event_bus
                .post(
                    event_bus::Priority::VeryHigh,
                    &Self::EVENT_SOURCE,
                    &self.state,
                )
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(())
    }
}
