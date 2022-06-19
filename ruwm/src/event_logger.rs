use core::fmt::Debug;

use log::info;

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::utils::asynch::channel::adapt::{adapt, dummy};

pub fn sink<D>(source: &'static str) -> impl Sender<Data = D> + 'static
where
    D: Send + Debug + 'static,
{
    adapt(dummy::<()>(), move |event| {
        info!("[{}] {:?}", source, event);
        None
    })
}

pub async fn process(mut receiver: impl Receiver<Data = impl Debug>) {
    loop {
        let event = receiver.recv().await;

        info!("Event: {:?}", event);
    }
}
