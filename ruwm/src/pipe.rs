use embedded_svc::channel::asyncs::{Receiver, Sender};

use crate::error;

pub async fn run<D>(
    receiver: impl Receiver<Data = D>,
    sender: impl Sender<Data = D>,
) -> error::Result<()> {
    run_transform(receiver, sender, |d| Some(d)).await
}

pub async fn run_transform<RD, SD>(
    mut receiver: impl Receiver<Data = RD>,
    mut sender: impl Sender<Data = SD>,
    transformer: impl Fn(RD) -> Option<SD>,
) -> error::Result<()> {
    loop {
        if let Some(value) = transformer(receiver.recv().await.map_err(error::svc)?) {
            sender.send(value).await.map_err(error::svc)?;
        }
    }
}
