use core::fmt::Debug;

use log::info;

use embedded_svc::channel::nonblocking::Receiver;

pub async fn run<R, E>(mut receiver: R)
where
    R: Receiver<Data = E>,
    E: Debug,
{
    loop {
        let event = receiver.recv().await;

        info!("Event: {:?}", event);
    }
}
