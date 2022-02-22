use core::convert::Infallible;
use core::future::Future;

use embedded_svc::channel::nonblocking::*;
use embedded_svc::errors::Errors;

pub fn broadcast<T>(
    cap: usize,
) -> anyhow::Result<(
    impl Sender<Data = T> + Clone,
    impl Receiver<Data = T> + Clone,
)>
where
    T: Send + Sync + Clone + 'static,
{
    let (sender, receiver) = async_broadcast::broadcast(cap);

    Ok((BroadcastSender(sender), BroadcastReceiver(receiver)))
}

#[derive(Clone)]
struct BroadcastSender<T>(async_broadcast::Sender<T>);

impl<T> Errors for BroadcastSender<T> {
    type Error = Infallible;
}

impl<T> Sender for BroadcastSender<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Data = T;

    type SendFuture<'a> = impl Future<Output = Result<(), Self::Error>>;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            self.0.broadcast(value).await.unwrap(); // TODO

            Ok(())
        }
    }
}

#[derive(Clone)]
struct BroadcastReceiver<T>(async_broadcast::Receiver<T>);

impl<T> Errors for BroadcastReceiver<T> {
    type Error = Infallible;
}

impl<T> Receiver for BroadcastReceiver<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Data = T;

    type RecvFuture<'a>
    where
        T: 'a,
    = impl Future<Output = Result<T, Self::Error>>;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move {
            let value = self.0.recv().await.unwrap(); // TODO

            Ok(value)
        }
    }
}
