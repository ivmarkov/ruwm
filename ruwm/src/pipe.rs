use core::fmt::Display;

use anyhow::anyhow;

use embedded_svc::channel::nonblocking::{Receiver, Sender};

pub async fn run<R, S, D>(receiver: R, sender: S) -> anyhow::Result<()>
where
    R: Receiver<Data = D>,
    S: Sender<Data = D>,
    R::Error: Display + Send + Sync + 'static,
    S::Error: Display + Send + Sync + 'static,
{
    run_transform(receiver, sender, |d| d).await
}

pub async fn run_transform<R, S, RD, SD>(
    mut receiver: R,
    mut sender: S,
    transformer: impl Fn(RD) -> SD,
) -> anyhow::Result<()>
where
    R: Receiver<Data = RD>,
    S: Sender<Data = SD>,
    R::Error: Display + Send + Sync + 'static,
    S::Error: Display + Send + Sync + 'static,
{
    loop {
        sender
            .send(transformer(receiver.recv().await.map_err(|e| anyhow!(e))?))
            .await
            .map_err(|e| anyhow!(e))?;
    }
}
