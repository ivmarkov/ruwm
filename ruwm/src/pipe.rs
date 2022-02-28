use embedded_svc::channel::nonblocking::{Receiver, Sender};

use crate::error;

pub async fn run<D>(
    receiver: impl Receiver<Data = D>,
    sender: impl Sender<Data = D>,
) -> error::Result<()> {
    run_transform(receiver, sender, |d| d).await
}

pub async fn run_transform<RD, SD>(
    mut receiver: impl Receiver<Data = RD>,
    mut sender: impl Sender<Data = SD>,
    transformer: impl Fn(RD) -> SD,
) -> error::Result<()> {
    loop {
        sender
            .send(transformer(receiver.recv().await.map_err(error::svc)?))
            .await
            .map_err(error::svc)?;
    }
}
