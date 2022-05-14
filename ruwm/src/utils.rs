use embedded_svc::channel::asyncs::{Sender, Receiver};
use embedded_svc::signal::asyncs::Signal;
use embedded_svc::utils::asyncs::signal::adapt::{as_sender, as_receiver};

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
