use core::{future::Future, marker::PhantomData};

use log::info;

use embassy_util::blocking_mutex::raw::RawMutex;

use crate::channel::{Receiver, Sender};
use crate::notification::Notification;
use crate::signal::Signal;
use crate::state::StateCellRead;

pub struct NotifReceiver<'a, S>(&'a Notification, &'a S);

impl<'a, S> NotifReceiver<'a, S> {
    pub const fn new(notif: &'a Notification, state: &'a S) -> Self {
        Self(notif, state)
    }
}

impl<'a, S> Receiver for NotifReceiver<'a, S>
where
    S: StateCellRead + Send + Sync + 'a,
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
    R: RawMutex + Send + Sync + 'a,
    T: Send + 'static,
{
    type Data = T;

    type RecvFuture<'b> = impl Future<Output = Self::Data> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move { self.0.wait().await }
    }
}

pub struct NotifSender<'a, const N: usize, P = ()>(
    [&'a Notification; N],
    &'static str,
    PhantomData<fn() -> P>,
);

impl<'a, const N: usize, P> NotifSender<'a, N, P> {
    pub const fn new(source: &'static str, notif: [&'a Notification; N]) -> Self {
        Self(notif, source, PhantomData)
    }
}

impl<'a, const N: usize, P> Sender for NotifSender<'a, N, P>
where
    P: core::fmt::Debug + Send,
{
    type Data = P;

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            info!("[{}] = {:?}", self.1, value);

            for notif in self.0 {
                notif.notify();
            }
        }
    }
}

// TODO: Fix this mess
pub struct NotifSender2<'a, const N: usize, const M: usize, P = ()>(
    [&'a Notification; N],
    [&'a Notification; M],
    &'static str,
    PhantomData<fn() -> P>,
);

impl<'a, const N: usize, const M: usize, P> NotifSender2<'a, N, M, P> {
    pub const fn new(
        source: &'static str,
        notif1: [&'a Notification; N],
        notif2: [&'a Notification; M],
    ) -> Self {
        Self(notif1, notif2, source, PhantomData)
    }
}

impl<'a, const N: usize, const M: usize, P> Sender for NotifSender2<'a, N, M, P>
where
    P: core::fmt::Debug + Send,
{
    type Data = P;

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            info!("[{}] = {:?}", self.2, value);

            for notif in self.0 {
                notif.notify();
            }

            for notif in self.1 {
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
    R: RawMutex + Send + Sync + 'a,
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

pub fn concat_arr<const N: usize, const L1: usize, const L2: usize, T>(
    arr1: [T; L1],
    arr2: [T; L2],
) -> [T; N] {
    IntoIterator::into_iter(arr1)
        .chain(IntoIterator::into_iter(arr2))
        .collect::<heapless::Vec<_, N>>()
        .into_array()
        .unwrap_or_else(|_| unreachable!())
}
