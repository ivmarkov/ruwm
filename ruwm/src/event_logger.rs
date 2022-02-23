use core::fmt::Debug;

use log::info;

use embedded_svc::channel::nonblocking::Receiver;

pub async fn run(mut receiver: impl Receiver<Data = impl Debug>) -> anyhow::Result<()> {
    loop {
        let event = receiver.recv().await;

        info!("Event: {:?}", event);
    }
}
