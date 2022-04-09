use embedded_svc::channel::asyncs::Receiver;

use crate::{broadcast_event::Quit, error};

pub async fn run(mut notif: impl Receiver<Data = Quit>) -> error::Result<()> {
    notif.recv().await.map_err(error::svc)?;

    Ok(())
}
