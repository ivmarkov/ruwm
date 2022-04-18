use embedded_svc::channel::asyncs::Receiver;

use crate::error;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Quit;

pub async fn run(mut notif: impl Receiver<Data = Quit>) -> error::Result<()> {
    notif.recv().await.map_err(error::svc)?;

    Ok(())
}
