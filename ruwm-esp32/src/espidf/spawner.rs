use embedded_svc::utils::asyncs::executor::{LocalExecutor, Notifier, Waiter};

use ruwm::broadcast_binder::{Spawner, TaskPriority};

pub struct ISRCompatibleLocalSpawner<'a, W, N>(LocalExecutor<'a, W, N>);

impl<'a, W, N> ISRCompatibleLocalSpawner<'a, W, N> {
    pub fn new(executor: LocalExecutor<'a, W, N>) -> Self {
        Self(executor)
    }

    pub fn executor(&mut self) -> &mut LocalExecutor<'a, W, N> {
        &mut self.0
    }
}

impl<'a, W, N> Spawner<'a> for ISRCompatibleLocalSpawner<'a, W, N>
where
    W: Waiter + 'a,
    N: Notifier + Clone + Send + 'a,
{
    fn spawn(
        &mut self,
        _priority: TaskPriority,
        fut: impl futures::Future<Output = ruwm::error::Result<()>> + 'a,
    ) -> ruwm::error::Result<()> {
        self.0.spawn(fut).detach();

        Ok(())
    }
}
