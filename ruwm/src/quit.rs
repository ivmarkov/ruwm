use embedded_svc::channel::asyncs::{Receiver, Sender};

use crate::error;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Quit;

pub async fn run(
    mut quit: impl Receiver<Data = Quit>,
    mut notif: impl Sender<Data = ()>,
) -> error::Result<()> {
    quit.recv().await.map_err(error::svc)?;
    notif.send(()).await.map_err(error::svc)?;

    Ok(())
}
