use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::signal::asynch::Signal;
use embedded_svc::utils::asynch::channel::adapt;
use embedded_svc::utils::asynch::signal::adapt::as_channel;

// Workaround, as we are possibly hit by this: https://github.com/rust-lang/rust/issues/64552
#[derive(Clone)]
pub struct StaticRef<C>(pub &'static C)
where
    C: 'static;

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
