use core::convert::Infallible;
use core::future::Future;

use embedded_svc::channel::asyncs::*;
use embedded_svc::errors::Errors;
use embedded_svc::utils::asyncs::signal;

use esp_idf_hal::mutex::Mutex;

use ruwm::broadcast_binder;
use ruwm::error;

pub struct SignalFactory;

impl<'a> broadcast_binder::SignalFactory<'a> for SignalFactory {
    type Sender<D>
    where
        D: 'a,
    = impl Sender<Data = D>;

    type Receiver<D>
    where
        D: 'a,
    = impl Receiver<Data = D>;

    fn create<D>(&mut self) -> error::Result<(Self::Sender<D>, Self::Receiver<D>)>
    where
        D: Send + Sync + Clone + 'a,
    {
        signal()
    }
}

pub fn signal<'a, T>() -> error::Result<(
    impl Sender<Data = T> + Clone,
    impl Receiver<Data = T> + Clone,
)>
where
    T: Send + Sync + Clone + 'a,
{
    let signal = signal::Signal::<Mutex<signal::State<T>>, T>::new();

    Ok((SignalSender(signal.clone()), SignalReceiver(signal)))
}

#[derive(Clone)]
struct SignalSender<T: Send>(signal::Signal<Mutex<signal::State<T>>, T>);

impl<T: Send> Errors for SignalSender<T> {
    type Error = Infallible;
}

impl<T> Sender for SignalSender<T>
where
    T: Clone + Send + Sync,
{
    type Data = T;

    type SendFuture<'a>
    where
        T: 'a,
    = impl Future<Output = Result<(), Self::Error>>;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            self.0.signal(value);

            Ok(())
        }
    }
}

#[derive(Clone)]
struct SignalReceiver<T: Send>(signal::Signal<Mutex<signal::State<T>>, T>);

impl<T: Send> Errors for SignalReceiver<T> {
    type Error = Infallible;
}

impl<T> Receiver for SignalReceiver<T>
where
    T: Clone + Send + Sync,
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
