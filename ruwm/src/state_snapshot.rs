use embedded_svc::channel::asyncs::Sender;
use embedded_svc::mutex::Mutex;

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
        updater: impl Fn(&S) -> S,
        notif: &mut impl Sender<Data = S>,
    ) -> bool
    where
        S: PartialEq + Clone,
    {
        let state = {
            let mut guard = self.0.lock();

            let state = updater(&guard);

            if *guard != state {
                *guard = state.clone();

                Some(state)
            } else {
                None
            }
        };

        if let Some(state) = state {
            notif.send(state).await;

            true
        } else {
            false
        }
    }

    pub async fn update(&self, state: S, state_sink: &mut impl Sender<Data = S>) -> bool
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
            state_sink.send(state).await;
        }

        updated
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
