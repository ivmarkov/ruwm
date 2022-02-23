use core::convert::Infallible;
use core::future::Future;

use embedded_svc::channel::nonblocking::*;
use embedded_svc::errors::Errors;
use embedded_svc::utils::nonblocking::signal;

use esp_idf_hal::mutex::Mutex;
use ruwm::broadcast_binder::Notif;

pub struct Notify;

impl Notif for Notify {
    type Sender<D> = impl Sender<Data = D>;

    type Receiver<D> = impl Receiver<Data = D>;

    fn create<D>(&mut self) -> anyhow::Result<(Self::Sender<D>, Self::Receiver<D>)>
    where
        D: Send + Sync + Clone + 'static,
    {
        notify()
    }
}

pub fn notify<T>() -> anyhow::Result<(
    impl Sender<Data = T> + Clone,
    impl Receiver<Data = T> + Clone,
)>
where
    T: Send + Sync + Clone + 'static,
{
    let signal = signal::Signal::<Mutex<signal::State<T>>, T>::new();

    Ok((NotifySender(signal.clone()), NotifyReceiver(signal)))
}

#[derive(Clone)]
struct NotifySender<T: Send>(signal::Signal<Mutex<signal::State<T>>, T>);

impl<T: Send> Errors for NotifySender<T> {
    type Error = Infallible;
}

impl<T> Sender for NotifySender<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Data = T;

    type SendFuture<'a> = impl Future<Output = Result<(), Self::Error>>;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            self.0.signal(value);

            Ok(())
        }
    }
}

#[derive(Clone)]
struct NotifyReceiver<T: Send>(signal::Signal<Mutex<signal::State<T>>, T>);

impl<T: Send> Errors for NotifyReceiver<T> {
    type Error = Infallible;
}

impl<T> Receiver for NotifyReceiver<T>
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
            let value = self.0.wait().await;

            Ok(value)
        }
    }
}
