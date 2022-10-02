use core::cell::RefCell;
use core::fmt::Debug;

use log::info;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;

use crate::notification::Notification;

pub struct State<'a, T, const N: usize> {
    name: &'a str,
    state: Mutex<CriticalSectionRawMutex, RefCell<T>>,
    notifications: [&'a Notification; N],
}

impl<'a, T, const N: usize> State<'a, T, N>
where
    T: Clone,
{
    pub const fn new(name: &'a str, data: T, notifications: [&'a Notification; N]) -> Self {
        Self {
            name,
            state: Mutex::new(RefCell::new(data)),
            notifications,
        }
    }

    pub fn get(&self) -> T {
        self.state.lock(|state| state.borrow().clone())
    }

    pub fn set(&self, data: T) -> T {
        self.state.lock(|state| {
            let old = state.borrow().clone();

            *state.borrow_mut() = data;

            old
        })
    }

    pub fn update_with(&self, updater: impl FnOnce(T) -> T) -> bool
    where
        T: PartialEq + Debug,
    {
        let old = self.set(updater(self.get()));
        let new = self.get();

        if old != new {
            info!("[{} STATE]: {:?}", self.name, new);

            for notification in self.notifications {
                notification.notify();
            }

            true
        } else {
            false
        }
    }

    pub fn update(&self, data: T) -> bool
    where
        T: PartialEq + Debug,
    {
        self.update_with(move |_| data)
    }
}
