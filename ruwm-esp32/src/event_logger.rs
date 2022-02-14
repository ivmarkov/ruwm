use log::info;

use embedded_svc::channel::nonblocking::Receiver;

use crate::event::Event;

pub async fn run<R>(mut receiver: R)
where
    R: Receiver<Data = Event>,
{
    loop {
        let event = receiver.recv().await;

        info!("Event: {:?}", event);
    }
}
