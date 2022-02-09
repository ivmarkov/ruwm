extern crate alloc;
use alloc::sync::Arc;

use embedded_svc::channel::nonblocking::Sender;
use embedded_svc::mutex::Mutex;

use crate::storage::Storage;

pub struct StateSnapshot<M>(Arc<M>);

impl<M, S> StateSnapshot<M>
where
    M: Mutex<Data = S>,
{
    pub fn new() -> Self
    where
        S: Default,
    {
        Self(Arc::new(M::new(Default::default())))
    }

    pub async fn update_with<N>(&self, updater: impl Fn(&S) -> S, notif: &mut N)
    where
        S: PartialEq + Clone,
        N: Sender<Data = S>,
    {
        let mut guard = self.0.lock();

        let state = updater(&guard);

        if *guard != state {
            *guard = state.clone();

            notif.send(state).await.unwrap();
        }
    }

    pub async fn update<N>(&self, state: S, notif: &mut N)
    where
        S: PartialEq + Clone,
        N: Sender<Data = S>,
    {
        let updated = {
            let mut guard = self.0.lock();

            if *guard != state {
                *guard = state.clone();
                true
            } else {
                false
            }
        };

        if updated {
            notif.send(state).await.unwrap();
        }
    }
}

impl<M> Clone for StateSnapshot<M>
where
    M: Send + Sync,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<M, S> Storage<S> for StateSnapshot<M>
where
    M: Mutex<Data = S>,
    S: Clone,
{
    fn get(&self) -> S {
        let guard = self.0.lock();

        guard.clone()
    }

    fn set(&mut self, data: S) {
        let mut guard = self.0.lock();

        *guard = data;
    }
}
