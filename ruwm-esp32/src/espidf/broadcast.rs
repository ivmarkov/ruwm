use core::future::Future;

use embedded_svc::channel::asyncs::*;
use embedded_svc::errors::Errors;
use embedded_svc::event_bus::asyncs::*;
use embedded_svc::utils::asyncify::event_bus::*;
use embedded_svc::utils::asyncify::Asyncify;

use esp_idf_hal::mutex::Condvar;

use esp_idf_svc::eventloop::*;

use esp_idf_sys::*;

use ruwm::error;

pub fn broadcast<D, T>(
    cap: usize,
) -> error::Result<(
    impl Sender<Data = T> + Clone,
    impl Receiver<Data = T> + Clone,
)>
where
    D: EspTypedEventSerializer<T> + EspTypedEventDeserializer<T> + Clone + 'static,
    T: Send + Sync + Clone + 'static,
{
    let mut blocking_event_bus = EspBackgroundEventLoop::new(&BackgroundLoopConfiguration {
        queue_size: cap,
        ..Default::default()
    })?
    .into_typed::<D, _>();

    let postbox = blocking_event_bus.as_async().postbox()?;

    let mut event_bus = blocking_event_bus.into_async();
    let subscription = event_bus.subscribe()?;

    Ok((
        BroadcastSender(postbox),
        BroadcastReceiver(event_bus, subscription),
    ))
}

#[derive(Clone)]
struct BroadcastSender<D, T>(AsyncPostbox<(), T, EspTypedPostbox<D, T, User<Background>>>);

impl<D, T> Errors for BroadcastSender<D, T> {
    type Error = EspError;
}

impl<D, T> Sender for BroadcastSender<D, T>
where
    D: EspTypedEventSerializer<T> + 'static,
    T: Clone + Send + Sync + 'static,
{
    type Data = T;

    type SendFuture<'a> = impl Future<Output = Result<(), Self::Error>> + Send;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        self.0.send(value)
    }
}

struct BroadcastReceiver<D, T: Send>(
    AsyncEventBus<(), Condvar, EspTypedEventLoop<D, T, EspBackgroundEventLoop>>,
    AsyncSubscription<Condvar, T, EspBackgroundSubscription, EspError>,
);

impl<D, T> Clone for BroadcastReceiver<D, T>
where
    T: Clone + Send + 'static,
    D: EspTypedEventDeserializer<T>,
{
    fn clone(&self) -> Self {
        let mut event_bus = self.0.clone();
        let subscription = event_bus.subscribe().unwrap();

        Self(event_bus, subscription)
    }
}

impl<D, T: Send> Errors for BroadcastReceiver<D, T> {
    type Error = EspError;
}

impl<D, T> Receiver for BroadcastReceiver<D, T>
where
    T: Clone + Send + Sync + 'static,
    D: EspTypedEventDeserializer<T> + 'static,
{
    type Data = T;

    type RecvFuture<'a>
    where
        T: 'a,
    = impl Future<Output = Result<T, Self::Error>> + Send;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        self.1.recv()
    }
}
