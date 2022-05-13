use embedded_svc::channel::asyncs::Sender;
use embedded_svc::mutex::Mutex;

use crate::error;
use crate::storage::Storage;

pub struct StateSnapshot<M>(M);

impl<M, S> StateSnapshot<M>
where
    M: Mutex<Data = S>,
{
    pub fn new() -> Self
    where
        S: Default,
    {
        Self(M::new(Default::default()))
    }

    pub async fn update_with(
        &self,
        updater: impl Fn(&S) -> error::Result<S>,
        notif: &mut impl Sender<Data = S>,
    ) -> error::Result<bool>
    where
        S: PartialEq + Clone,
    {
        let state = {
            let mut guard = self.0.lock();

            let state = updater(&guard)?;

            if *guard != state {
                *guard = state.clone();

                Some(state)
            } else {
                None
            }
        };

        if let Some(state) = state {
            notif.send(state).await.map_err(error::svc)?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn update(&self, state: S, notif: &mut impl Sender<Data = S>) -> error::Result<bool>
    where
        S: PartialEq + Clone,
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
            notif.send(state).await.map_err(error::svc)?;
        }

        Ok(updated)
    }
}

impl<M, S> Default for StateSnapshot<M>
where
    M: Mutex<Data = S>,
    S: Default,
{
    fn default() -> Self {
        Self::new()
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
