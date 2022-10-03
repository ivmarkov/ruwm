use core::cell::RefCell;
use core::fmt::Debug;

use log::info;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;

use crate::notification::Notification;

pub struct State<'a, T, const N1: usize, const N2: usize> {
    name: &'a str,
    state: Mutex<CriticalSectionRawMutex, RefCell<T>>,
    notifications1: [&'a Notification; N1],
    notifications2: &'a [Notification; N2],
}

impl<'a, T, const N1: usize, const N2: usize> State<'a, T, N1, N2>
where
    T: Clone,
{
    pub const fn new(
        name: &'a str,
        data: T,
        notifications1: [&'a Notification; N1],
        notifications2: &'a [Notification; N2],
    ) -> Self {
        Self {
            name,
            state: Mutex::new(RefCell::new(data)),
            notifications1,
            notifications2,
        }
    }

    pub fn get(&self) -> T {
        self.state.lock(|state| state.borrow().clone())
    }

    pub fn set(&self, data: T) -> (T, T) {
        self.state.lock(|state| {
            let old = state.borrow().clone();

            *state.borrow_mut() = data.clone();

            (old, data)
        })
    }

    pub fn set_update(&self, updater: impl FnOnce(T) -> T) -> (T, T) {
        self.state.lock(|state| {
            let old = state.borrow().clone();
            let new = updater(old.clone());

            *state.borrow_mut() = new.clone();

            (old, new)
        })
    }

    pub fn update_with(&self, updater: impl FnOnce(T) -> T) -> bool
    where
        T: PartialEq + Debug,
    {
        let (old, new) = self.set_update(updater);

        if old != new {
            info!("[{} STATE]: {:?}", self.name, new);

            for notification in self.notifications1 {
                notification.notify();
            }

            for notification in self.notifications2 {
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
