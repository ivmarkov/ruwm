use crate::channel::{Receiver, Sender};

pub async fn process<D>(receiver: impl Receiver<Data = D>, sender: impl Sender<Data = D>) {
    process_transform(receiver, sender, |d| Some(d)).await
}

pub async fn process_transform<RD, SD>(
    mut receiver: impl Receiver<Data = RD>,
    mut sender: impl Sender<Data = SD>,
    transformer: impl Fn(RD) -> Option<SD>,
) {
    loop {
        if let Some(value) = transformer(receiver.recv().await) {
            sender.send(value).await;
        }
    }
}
