use futures::{executor::LocalPool, task::LocalSpawnExt, FutureExt};

use smol::LocalExecutor;

use ruwm::broadcast_binder::{Spawner, TaskPriority};

pub struct SmolLocalSpawner<'a>(LocalExecutor<'a>);

impl<'a> SmolLocalSpawner<'a> {
    pub fn new(executor: LocalExecutor<'a>) -> Self {
        Self(executor)
    }

    pub fn executor(&mut self) -> &mut LocalExecutor<'a> {
        &mut self.0
    }
}

impl<'a> Spawner<'a> for SmolLocalSpawner<'a> {
    fn spawn(
        &mut self,
        _priority: TaskPriority,
        fut: impl futures::Future<Output = ruwm::error::Result<()>> + 'a,
    ) -> ruwm::error::Result<()> {
        self.0.spawn(fut).detach();

        Ok(())
    }
}

pub struct FuturesLocalSpawner(LocalPool);

impl FuturesLocalSpawner {
    pub fn new(pool: LocalPool) -> Self {
        Self(pool)
    }

    pub fn pool(&mut self) -> &mut LocalPool {
        &mut self.0
    }
}

impl Spawner<'static> for FuturesLocalSpawner {
    fn spawn(
        &mut self,
        _priority: TaskPriority,
        fut: impl futures::Future<Output = ruwm::error::Result<()>> + 'static,
    ) -> ruwm::error::Result<()> {
        self.0.spawner().spawn_local(fut.map(|r| r.unwrap()))?;

        Ok(())
    }
}
