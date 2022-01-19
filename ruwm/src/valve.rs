use core::cell::RefCell;
use core::time::Duration;

extern crate alloc;
use alloc::rc::Rc;

use embedded_svc::event_bus;
use embedded_svc::timer;
use embedded_svc::timer::Timer;

use crate::storage::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValveState {
    Open,
    Closed,
    Opening,
    Closing,
}

pub struct Valve<S, T, P>
where
    T: timer::PinnedTimerService,
{
    timer: T::Timer,
    storage: S,
    postbox: P,
    state: Rc<RefCell<Option<ValveState>>>,
}

impl<S, T, P> Valve<S, T, P>
where
    S: Storage<Option<ValveState>>,
    T: timer::PinnedTimerService,
    P: event_bus::Postbox + 'static,
{
    pub const EVENT_SOURCE: event_bus::Source<Option<ValveState>> =
        event_bus::Source::new(b"VALVE\0");

    pub fn new(
        timer_service: &T,
        mut postbox1: P,
        postbox2: P,
        storage: S,
    ) -> anyhow::Result<Self> {
        let state = Rc::new(RefCell::new(storage.get()));
        let state_timer = Rc::downgrade(&state);

        let timer = timer_service
            .timer(&Default::default(), move || {
                state_timer
                    .upgrade()
                    .map(|s| Self::complete(&mut postbox1, &mut s.borrow_mut()))
                    .unwrap_or(Ok(()))
            })
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(Valve {
            timer,
            storage,
            postbox: postbox2,
            state,
        })
    }

    pub fn state(&self) -> Option<ValveState> {
        *self.state.borrow()
    }

    pub fn open(&mut self, force: bool) -> anyhow::Result<()> {
        let mut ss = self.state.borrow_mut();

        if *ss != Some(ValveState::Open) || force {
            Self::set_state(&mut self.postbox, &mut ss, Some(ValveState::Opening))?;

            self.timer
                .once(Duration::from_secs(20))
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(())
    }

    pub fn close(&mut self, force: bool) -> anyhow::Result<()> {
        let mut ss = self.state.borrow_mut();

        if *ss != Some(ValveState::Closed) || force {
            Self::set_state(&mut self.postbox, &mut ss, Some(ValveState::Closing))?;

            self.timer
                .once(Duration::from_secs(20))
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(())
    }

    fn complete<PP>(poster: &PP, ss: &mut Option<ValveState>) -> anyhow::Result<()>
    where
        PP: event_bus::Postbox,
    {
        let state = match *ss {
            Some(ValveState::Opening) => Some(ValveState::Open),
            Some(ValveState::Closing) => Some(ValveState::Closed),
            other => other,
        };

        Self::set_state(poster, ss, state)
    }

    fn set_state<PP>(
        poster: &PP,
        ss: &mut Option<ValveState>,
        state: Option<ValveState>,
    ) -> anyhow::Result<()>
    where
        PP: event_bus::Postbox,
    {
        if *ss != state {
            *ss = state;

            poster
                .post(&Self::EVENT_SOURCE, &*ss)
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        Ok(())
    }
}
