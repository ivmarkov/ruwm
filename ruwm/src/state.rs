use core::cell::RefCell;
use core::fmt::Debug;

use log::info;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;

use crate::channel::Sender;

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

    pub fn silent_update<F>(&self, updater: F) -> (T, T)
    where
        F: FnOnce(T) -> T,
    {
        let old = self.set(updater(self.get()));
        let new = self.get();

        (old, new)
    }

    pub async fn update_with(
        &self,
        state_name: &'static str,
        updater: impl FnOnce(T) -> T,
        mut notif: impl Sender<Data = ()>,
    ) -> bool
    where
        T: PartialEq + Debug,
    {
        let (old, new) = self.silent_update(updater);

        if old != new {
            info!("[{} STATE]: {:?}", state_name, new);

            notif.send(()).await;

            true
        } else {
            false
        }
    }

    pub async fn update(
        &self,
        state_name: &'static str,
        data: T,
        notif: impl Sender<Data = ()>,
    ) -> bool
    where
        T: PartialEq + Debug,
    {
        self.update_with(state_name, move |_| data, notif).await
    }
}
