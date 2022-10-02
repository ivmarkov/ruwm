use core::cell::RefCell;
use core::fmt::Debug;

use log::info;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;

use crate::notification::{notify_all, Notification};

pub struct State<T>(Mutex<CriticalSectionRawMutex, RefCell<T>>);

impl<T> State<T>
where
    T: Clone,
{
    pub const fn new(data: T) -> Self {
        Self(Mutex::new(RefCell::new(data)))
    }

    pub fn get(&self) -> T {
        self.0.lock(|state| state.borrow().clone())
    }

    pub fn set(&self, data: T) -> T {
        self.0.lock(|state| {
            let old = state.borrow().clone();

            *state.borrow_mut() = data;

            old
        })
    }

    pub fn update_with<'a>(
        &self,
        state_name: &'static str,
        updater: impl FnOnce(T) -> T,
        notifications: impl IntoIterator<Item = &'a &'a Notification>,
    ) -> bool
    where
        T: PartialEq + Debug,
    {
        let old = self.set(updater(self.get()));
        let new = self.get();

        if old != new {
            info!("[{} STATE]: {:?}", state_name, new);

            notify_all(notifications);

            true
        } else {
            false
        }
    }

    pub fn update<'a>(
        &self,
        state_name: &'static str,
        data: T,
        notifications: impl IntoIterator<Item = &'a &'a Notification>,
    ) -> bool
    where
        T: PartialEq + Debug,
    {
        self.update_with(state_name, move |_| data, notifications)
    }
}
