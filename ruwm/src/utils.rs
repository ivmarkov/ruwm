use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::signal::asynch::Signal;
use embedded_svc::utils::asynch::channel::adapt;
use embedded_svc::utils::asynch::signal::adapt::as_channel;
use embedded_svc::utils::asynch::signal::AtomicSignal;

// TODO: Something seems wrong here as this signature should
// be equivalent to as_channel which is being called
// Late-binding lifetimes?
pub fn as_static_sender<S, T>(signal: &'static S) -> impl Sender<Data = T> + 'static
where
    S: Signal<Data = T> + Send + Sync + 'static,
    T: Send + 'static,
{
    as_channel(signal)
}

pub struct AtomicSignalW(pub &'static AtomicSignal<()>);

// TODO: Something seems wrong here as this signature should
// be equivalent to as_channel which is being called
// Late-binding lifetimes?
pub fn as_static_receiver<S, T>(signal: &'static S) -> impl Receiver<Data = T> + 'static
where
    S: Signal<Data = T> + Send + Sync + 'static,
    T: Send + 'static,
{
    as_channel(signal)
}

pub fn as_static_receiver2(signal: AtomicSignalW) -> impl Receiver<Data = ()> + 'static {
    as_channel(signal.0)
}

// TODO: Something seems wrong here as this signature should
// be equivalent to adapt which is being called
// Late-binding lifetimes?
pub fn adapt_static_receiver<R, T, F>(receiver: R, adapter: F) -> impl Receiver<Data = T> + 'static
where
    R: Receiver + Send + 'static,
    R::Data: 'static,
    T: Send + 'static,
    F: Fn(R::Data) -> Option<T> + Send + Sync + 'static,
{
    adapt::adapt(receiver, adapter)
}
