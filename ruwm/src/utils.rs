use core::future::Future;

use log::info;

use embassy_util::blocking_mutex::raw::RawMutex;

use embedded_svc::channel::asynch::{Receiver, Sender};

use crate::{notification::Notification, signal::Signal, state::StateCellRead};

pub struct NotifReceiver<'a, S>(&'a Notification, &'a S);

impl<'a, S> NotifReceiver<'a, S> {
    pub const fn new(notif: &'a Notification, state: &'a S) -> Self {
        Self(notif, state)
    }
}

impl<'a, S> Receiver for NotifReceiver<'a, S>
where
    S: StateCellRead + Send + Sync,
    S::Data: Send,
{
    type Data = S::Data;

    type RecvFuture<'b> = impl Future<Output = Self::Data> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move {
            self.0.wait();

            self.1.get()
        }
    }
}

pub struct SignalReceiver<'a, R, T>(&'a Signal<R, T>)
where
    R: RawMutex;

impl<'a, R, T> SignalReceiver<'a, R, T>
where
    R: RawMutex,
{
    pub const fn new(signal: &'a Signal<R, T>) -> Self {
        Self(signal)
    }
}

impl<'a, R, T> Receiver for SignalReceiver<'a, R, T>
where
    R: RawMutex + Send + Sync,
    T: Send + 'static,
{
    type Data = T;

    type RecvFuture<'b> = impl Future<Output = Self::Data> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move { self.0.wait().await }
    }
}

pub struct NotifSender<'a, const N: usize>([&'a Notification; N], &'static str);

impl<'a, const N: usize> NotifSender<'a, N> {
    pub const fn new(source: &'static str, notif: [&'a Notification; N]) -> Self {
        Self(notif, source)
    }
}

impl<'a, const N: usize> Sender for NotifSender<'a, N> {
    type Data = ();

    type SendFuture<'b> = impl Future<Output = Self::Data>
    where Self: 'b;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            info!("{}", self.1);

            for notif in self.0 {
                notif.notify();
            }
        }
    }
}

pub struct SignalSender<'a, const N: usize, R, T>([&'a Signal<R, T>; N], &'static str)
where
    R: RawMutex;

impl<'a, const N: usize, R, T> SignalSender<'a, N, R, T>
where
    R: RawMutex,
{
    pub const fn new(source: &'static str, signal: [&'a Signal<R, T>; N]) -> Self {
        Self(signal, source)
    }
}

impl<'a, const N: usize, R, T> Sender for SignalSender<'a, N, R, T>
where
    R: RawMutex + Send + Sync,
    T: Send + Clone + 'static,
{
    type Data = T;

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            for signal in self.0 {
                signal.signal(value.clone());
            }

            info!("{}", self.1);
        }
    }
}

pub fn as_arr<T, const N: usize, const M: usize, const R: usize>(
    arr1: [T; N],
    arr2: [T; M],
) -> [T; R]
where
    T: Default,
{
    let result = [Default::default(); R];

    for index in 0..N {
        result[index] = arr1[index];
    }

    for index in 0..M {
        result[N + index] = arr2[index];
    }

    result
}
