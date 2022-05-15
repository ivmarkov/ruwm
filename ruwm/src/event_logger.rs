use core::fmt::Debug;

use log::info;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::utils::asyncs::channel::adapt::{dummy, sender};

use crate::error;

pub fn sink<D>() -> impl Sender<Data = D> + 'static
where
    D: Send + Debug + 'static,
{
    sender(dummy::<()>(), |event| {
        info!("Event: {:?}", event);
        None
    })
}

pub async fn process(mut receiver: impl Receiver<Data = impl Debug>) -> error::Result<()> {
    loop {
        let event = receiver.recv().await;

        info!("Event: {:?}", event);
    }
}
