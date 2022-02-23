use anyhow::anyhow;

use embedded_svc::channel::nonblocking::{Receiver, Sender};

pub async fn run<D>(
    receiver: impl Receiver<Data = D>,
    sender: impl Sender<Data = D>,
) -> anyhow::Result<()> {
    run_transform(receiver, sender, |d| d).await
}

pub async fn run_transform<RD, SD>(
    mut receiver: impl Receiver<Data = RD>,
    mut sender: impl Sender<Data = SD>,
    transformer: impl Fn(RD) -> SD,
) -> anyhow::Result<()> {
    loop {
        sender
            .send(transformer(receiver.recv().await.map_err(|e| anyhow!(e))?))
            .await
            .map_err(|e| anyhow!(e))?;
    }
}
