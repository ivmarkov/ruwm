use futures::future::Either;

use embedded_svc::channel::nonblocking::{Receiver, Sender};

pub async fn run<R, S, D>(receiver: R, sender: S) -> Either<R::Error, S::Error>
where
    R: Receiver<Data = D>,
    S: Sender<Data = D>,
{
    run_transform(receiver, sender, |d| d).await
}

pub async fn run_transform<R, S, RD, SD>(
    mut receiver: R,
    mut sender: S,
    transformer: impl Fn(RD) -> SD,
) -> Either<R::Error, S::Error>
where
    R: Receiver<Data = RD>,
    S: Sender<Data = SD>,
{
    loop {
        let err = match receiver.recv().await {
            Ok(data) => match sender.send(transformer(data)).await {
                Ok(_) => None,
                Err(err) => Some(Either::Right(err)),
            },
            Err(err) => Some(Either::Left(err)),
        };

        if let Some(err) = err {
            return err;
        }
    }
}
