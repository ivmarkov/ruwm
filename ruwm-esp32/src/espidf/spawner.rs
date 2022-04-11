use embedded_svc::utils::asyncs::executor::{LocalExecutor, Notifier, Waiter};
use ruwm::broadcast_binder::Spawner;

pub struct ISRCompatibleLocalSpawner<'a, const S: usize, W, N>(LocalExecutor<'a, S, W, N>);

impl<'a, const S: usize, W, N> ISRCompatibleLocalSpawner<'a, S, W, N> {
    pub fn new(executor: LocalExecutor<'a, S, W, N>) -> Self {
        Self(executor)
    }

    pub fn executor(&mut self) -> &mut LocalExecutor<'a, S, W, N> {
        &mut self.0
    }
}

impl<'a, const S: usize, W, N> Spawner<'a> for ISRCompatibleLocalSpawner<'a, S, W, N>
where
    W: Waiter,
    N: Notifier,
{
    fn spawn(
        &mut self,
        fut: impl futures::Future<Output = ruwm::error::Result<()>> + 'a,
    ) -> ruwm::error::Result<()> {
        self.0.spawn(fut).detach();

        Ok(())
    }
}
