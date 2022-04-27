use embedded_svc::executor::asyncs::Spawner;
use futures::{executor::LocalPool, future::RemoteHandle, task::LocalSpawnExt};

use smol::{LocalExecutor, Task};

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
    type Task<T>
    where
        T: 'a,
    = Task<T>;

    fn spawn<F, T>(&mut self, fut: F) -> Self::Task<T>
    where
        F: futures::Future<Output = T> + Send + 'a,
        T: 'a,
    {
        self.0.spawn(fut)
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
    type Task<T>
    where
        T: 'static,
    = RemoteHandle<T>;

    fn spawn<F, T>(&mut self, fut: F) -> Self::Task<T>
    where
        F: futures::Future<Output = T> + Send + 'static,
        T: 'static,
    {
        self.0.spawner().spawn_local_with_handle(fut).unwrap()
    }
}
