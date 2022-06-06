use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::signal::asynch::Signal;
use embedded_svc::utils::asynch::signal::adapt::as_channel;

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
