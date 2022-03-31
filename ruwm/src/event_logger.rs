use core::fmt::Debug;

use log::info;

use embedded_svc::channel::asyncs::Receiver;

use crate::error;

pub async fn run(mut receiver: impl Receiver<Data = impl Debug>) -> error::Result<()> {
    loop {
        let event = receiver.recv().await;

        info!("Event: {:?}", event);
    }
}
