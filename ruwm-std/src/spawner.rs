use std::convert::Infallible;

use futures::{
    executor::LocalPool,
    future::RemoteHandle,
    task::{LocalSpawnExt, SpawnError},
};

use smol::{LocalExecutor, Task};

use embedded_svc::executor::asynch::Spawner;

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
    type Error = Infallible;

    type Task<T>
    = Task<T>
    where
        T: 'a;

    fn spawn<F, T>(&mut self, fut: F) -> Result<Self::Task<T>, Self::Error>
    where
        F: futures::Future<Output = T> + Send + 'a,
        T: 'a,
    {
        Ok(self.0.spawn(fut))
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
    type Error = SpawnError;

    type Task<T>
    = RemoteHandle<T>
    where
        T: 'static;

    fn spawn<F, T>(&mut self, fut: F) -> Result<Self::Task<T>, Self::Error>
    where
        F: futures::Future<Output = T> + Send + 'static,
        T: 'static,
    {
        self.0.spawner().spawn_local_with_handle(fut)
    }
}
