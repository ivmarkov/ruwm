use core::cell::UnsafeCell;
use core::ops::Deref;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::signal::asyncs::Signal;
use embedded_svc::utils::asyncs::signal::adapt::{as_receiver, as_sender};

pub struct AlmostOnce<T>(UnsafeCell<Option<T>>);

impl<T> AlmostOnce<T> {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(None))
    }

    pub fn init(&self, value: T) {
        let mut_ref = unsafe { self.0.get().as_mut().unwrap() };
        *mut_ref = Some(value);
    }
}

impl<T> Deref for AlmostOnce<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.get().as_ref().unwrap() }.as_ref().unwrap()
    }
}

unsafe impl<T> Sync for AlmostOnce<T> {}

// TODO: Something seems wrong with rustc here as this signature should
// be equivalent to as_receiver() from above
pub fn as_static_sender<S, T>(signal: &'static S) -> impl Sender<Data = T> + 'static
where
    S: Signal<Data = T> + Send + Sync,
    T: Send + 'static,
{
    as_sender(signal)
}

// TODO: Something seems wrong with rustc here as this signature should
// be equivalent to as_receiver() from above
pub fn as_static_receiver<S, T>(signal: &'static S) -> impl Receiver<Data = T> + 'static
where
    S: Signal<Data = T> + Send + Sync + 'static,
    T: Send + 'static,
{
    as_receiver(signal)
}
