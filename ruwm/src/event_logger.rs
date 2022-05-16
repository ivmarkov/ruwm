use core::fmt::Debug;

use log::info;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::utils::asyncs::channel::adapt::{adapt, dummy};

use crate::error;

pub fn sink<D>(source: &'static str) -> impl Sender<Data = D> + 'static
where
    D: Send + Debug + 'static,
{
    adapt(dummy::<()>(), move |event| {
        info!("[{}] {:?}", source, event);
        None
    })
}

pub async fn process(mut receiver: impl Receiver<Data = impl Debug>) -> error::Result<()> {
    loop {
        let event = receiver.recv().await;

        info!("Event: {:?}", event);
    }
}
