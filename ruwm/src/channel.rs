use core::fmt::Debug;
use core::{future::Future, marker::PhantomData};

use embassy_sync::{blocking_mutex::raw::RawMutex, signal::Signal};
use log::info;

use crate::notification::Notification;
use crate::state::State;

pub trait Sender {
    type Data: Send;

    type SendFuture<'a>: Future
    where
        Self: 'a;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_>;
}

impl<S> Sender for &mut S
where
    S: Sender,
{
    type Data = S::Data;

    type SendFuture<'a>
    = S::SendFuture<'a> where Self: 'a;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        (*self).send(value)
    }
}

pub trait Receiver {
    type Data: Send;

    type RecvFuture<'a>: Future<Output = Self::Data>
    where
        Self: 'a;

    fn recv(&mut self) -> Self::RecvFuture<'_>;
}

impl<R> Receiver for &mut R
where
    R: Receiver,
{
    type Data = R::Data;

    type RecvFuture<'a>
    = R::RecvFuture<'a> where Self: 'a;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        (*self).recv()
    }
}

pub struct Channel<C>(C);

impl<C> Channel<C> {
    pub const fn new(c: C) -> Self {
        Self(c)
    }
}

//
// Tuple and array adapters
//

impl<S1, S2> Sender for (S1, S2)
where
    S1: Sender,
    S1::Data: Clone,
    S2: Sender<Data = S1::Data>,
{
    type Data = S1::Data;

    type SendFuture<'a> = impl Future<Output = ()> where Self: 'a;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            self.0.send(value.clone()).await;
            self.1.send(value).await;
        }
    }
}

impl<S1, S2, S3> Sender for (S1, S2, S3)
where
    S1: Sender,
    S1::Data: Clone,
    S2: Sender<Data = S1::Data>,
    S3: Sender<Data = S1::Data>,
{
    type Data = S1::Data;

    type SendFuture<'a> = impl Future<Output = ()> where Self: 'a;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move { ((&mut self.0, &mut self.1), &mut self.2).send(value).await }
    }
}

impl<const N: usize, S> Sender for [S; N]
where
    S: Sender,
    S::Data: Clone,
{
    type Data = S::Data;

    type SendFuture<'a> = impl Future<Output = ()> where Self: 'a;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            for sender in self {
                sender.send(value.clone()).await;
            }
        }
    }
}

//
// Notification adapters
//

impl<'a> Receiver for &'a Notification {
    type Data = ();

    type RecvFuture<'b> = impl Future<Output = Self::Data> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move { self.wait().await }
    }
}

impl<'a> Sender for &'a Notification {
    type Data = ();

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, _value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            self.notify();
        }
    }
}

impl<'a> Sender for &'a [&'a Notification] {
    type Data = ();

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, _value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            for notification in self.iter() {
                notification.notify();
            }
        }
    }
}

impl<'a, T> Receiver for (&'a Notification, &'a State<T>)
where
    T: Clone + Send + 'a,
{
    type Data = T;

    type RecvFuture<'b> = impl Future<Output = Self::Data> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move {
            self.0.wait().await;

            self.1.get()
        }
    }
}

impl<'a, P> Sender for (&'a Notification, PhantomData<fn() -> P>)
where
    P: core::fmt::Debug + Send,
{
    type Data = P;

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, _value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            self.0.notify();
        }
    }
}

pub type NotifSender<'a, P> = (&'a Notification, PhantomData<fn() -> P>);

impl<'a, P> From<&'a Notification> for (&'a Notification, PhantomData<fn() -> P>) {
    fn from(notification: &'a Notification) -> Self {
        (notification, PhantomData)
    }
}

//
// Signal adapters
//

impl<'a, R, T> Receiver for &'a Signal<R, T>
where
    R: RawMutex + Send + Sync + 'a,
    T: Send + 'static,
{
    type Data = T;

    type RecvFuture<'b> = impl Future<Output = Self::Data> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move { self.wait().await }
    }
}

impl<'a, R, T> Sender for &'a Signal<R, T>
where
    R: RawMutex + Send + Sync + 'a,
    T: Send + Clone + Debug + 'static,
{
    type Data = T;

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            self.signal(value);
        }
    }
}

//
// EventBus adapter
//

impl<R, T> Receiver for Channel<R>
where
    R: embedded_svc::event_bus::asynch::Receiver<Data = T> + Send,
    T: Send + 'static,
{
    type Data = T;

    type RecvFuture<'b> = impl Future<Output = Self::Data> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move { self.0.recv().await }
    }
}

//
// Log adapter
//

pub struct LogSender<'a, T> {
    prefix: &'a str,
    data: PhantomData<T>,
}

impl<'a, T> LogSender<'a, T> {
    pub const fn new(prefix: &'a str) -> Self {
        Self {
            prefix,
            data: PhantomData,
        }
    }
}

impl<'a, T> Sender for LogSender<'a, T>
where
    T: Debug + Send,
{
    type Data = T;

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            info!("[{}]: {:?}", self.prefix, value);
        }
    }
}
