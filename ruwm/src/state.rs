use core::marker::PhantomData;

use serde::{de::DeserializeOwned, Serialize};

use embedded_svc::{
    channel::asynch::Sender,
    mutex::{Mutex, SingleThreadedMutex},
    storage::SerDe,
};

pub trait StateCellRead {
    type Data;

    fn get(&self) -> Self::Data;
}

pub trait StateCell: StateCellRead {
    fn set(&self, data: Self::Data) -> Self::Data;

    fn update<F>(&self, updater: F) -> (Self::Data, Self::Data)
    where
        F: FnOnce(Self::Data) -> Self::Data,
    {
        (self.get(), self.set(updater(self.get())))
    }
}

pub async fn update_with<S, D>(
    state: &S,
    updater: impl FnOnce(D) -> D,
    notif: &mut impl Sender<Data = ()>,
) -> bool
where
    S: StateCell<Data = D>,
    D: PartialEq,
{
    let (old, new) = state.update(updater);

    if old != new {
        notif.send(());

        true
    } else {
        false
    }
}

pub async fn update<S, D>(state: &S, data: D, notif: &mut impl Sender<Data = ()>) -> bool
where
    S: StateCell<Data = D>,
    D: PartialEq,
{
    update_with(state, move |_| data, notif).await
}

pub type STMemoryStateCell<D> = MemoryStateCell<SingleThreadedMutex<D>>;

pub struct MemoryStateCell<M>(M);

impl<M, D> MemoryStateCell<M>
where
    M: Mutex<Data = D>,
{
    pub fn new(data: D) -> Self {
        Self(M::new(data))
    }
}

impl<M, D> StateCellRead for MemoryStateCell<M>
where
    M: Mutex<Data = D>,
    D: Clone,
{
    type Data = D;

    fn get(&self) -> Self::Data {
        self.0.lock().clone()
    }
}

impl<M, D> StateCell for MemoryStateCell<M>
where
    M: Mutex<Data = D>,
    D: Clone,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let mut guard = self.0.lock();

        let old_data = (&*guard).clone();

        *guard = data;

        old_data
    }
}

pub type STMutRefStateCell<D> = MutRefStateCell<SingleThreadedMutex<&'static mut D>>;

pub struct MutRefStateCell<M>(M);

impl<M, D> MutRefStateCell<M>
where
    M: Mutex<Data = &'static mut D>,
    D: 'static,
{
    pub fn new(data: &'static mut D) -> Self {
        Self(M::new(data))
    }
}

impl<M, D> StateCellRead for MutRefStateCell<M>
where
    M: Mutex<Data = &'static mut D>,
    D: Clone + 'static,
{
    type Data = D;

    fn get(&self) -> Self::Data {
        self.0.lock().clone()
    }
}

impl<M, D> StateCell for MutRefStateCell<M>
where
    M: Mutex<Data = &'static mut D>,
    D: Clone + 'static,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let mut guard = self.0.lock();

        let old_data = (&**guard).clone();

        **guard = data;

        old_data
    }
}

pub type STCachingStateCell<C, S> = CachingStateCell<SingleThreadedMutex<(C, S)>>;

pub struct CachingStateCell<M>(M);

impl<M, C, S> CachingStateCell<M>
where
    M: Mutex<Data = (C, S)>,
{
    pub fn new(cache: C, state: S) -> Self {
        Self(M::new((cache, state)))
    }
}

impl<M, C, S> StateCellRead for CachingStateCell<M>
where
    M: Mutex<Data = (C, S)>,
    C: StateCell<Data = Option<S::Data>>,
    S: StateCell,
    S::Data: Clone,
{
    type Data = S::Data;

    fn get(&self) -> Self::Data {
        let mut guard = self.0.lock();

        if let Some(data) = guard.0.get() {
            data
        } else {
            let data = guard.1.get();

            guard.0.set(Some(data.clone()));

            data
        }
    }
}

impl<C, S, M> StateCell for CachingStateCell<M>
where
    M: Mutex<Data = (C, S)>,
    C: StateCell<Data = Option<S::Data>>,
    S: StateCell,
    S::Data: Clone + PartialEq,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let guard = self.0.lock();

        let old_data = guard.0.get();

        if let Some(old_data) = old_data {
            if old_data != data {
                guard.0.set(Some(data.clone()));
                guard.1.set(data);
            }

            old_data
        } else {
            guard.0.set(Some(data.clone()));

            guard.1.set(data)
        }
    }
}

pub type STWearLevelingStateCell<C> = WearLevelingStateCell<SingleThreadedMutex<(C, usize)>>;

pub struct WearLevelingStateCell<M>(M);

impl<M, C> StateCellRead for WearLevelingStateCell<M>
where
    M: Mutex<Data = (C, usize, usize)>,
    C: StateCell,
{
    type Data = C::Data;

    fn get(&self) -> Self::Data {
        self.0.lock().0.get()
    }
}

impl<M, C> StateCell for WearLevelingStateCell<M>
where
    M: Mutex<Data = (C, usize, usize)>,
    C: StateCell,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let mut guard = self.0.lock();

        if guard.1 >= guard.2 {
            guard.1 = 0;

            guard.0.set(data)
        } else {
            guard.1 += 1;

            guard.0.get()
        }
    }
}

pub struct StorageStateCell<'a, M, D> {
    storage: &'a M,
    name: &'a str,
    _data: PhantomData<D>,
}

impl<'a, M, D> StorageStateCell<'a, M, D> {
    fn new(storage: &'a M, name: &'a str) -> Self {
        Self {
            storage,
            name,
            _data: PhantomData,
        }
    }
}

impl<'a, M, S, D> StateCellRead for StorageStateCell<'a, M, D>
where
    M: Mutex<Data = S>,
    S: embedded_svc::storage::Storage,
    D: Serialize + DeserializeOwned,
{
    type Data = D;

    fn get(&self) -> Self::Data {
        self.storage.lock().get(self.name).unwrap().unwrap()
    }
}

impl<'a, M, S, D> StateCell for StorageStateCell<'a, M, D>
where
    M: Mutex<Data = S>,
    S: embedded_svc::storage::Storage,
    D: Serialize + DeserializeOwned,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let mut guard = self.storage.lock();

        let old_data = guard.get(self.name).unwrap().unwrap();

        guard.set(self.name, &data).unwrap();

        old_data
    }
}

pub type PostcardStorageStateCell<'a, const N: usize, R, D> =
    StorageStateCell<'a, PostcardStorage<N, R>, D>;

pub type PostcardStorage<const N: usize, R> =
    embedded_svc::storage::StorageImpl<N, R, PostcardSerDe>;

pub struct PostcardSerDe;

impl SerDe for PostcardSerDe {
    type Error = postcard::Error;

    fn serialize<'a, T>(&self, slice: &'a mut [u8], value: &T) -> Result<&'a [u8], Self::Error>
    where
        T: Serialize,
    {
        postcard::to_slice(value, slice).map(|r| &*r)
    }

    fn deserialize<T>(&self, slice: &[u8]) -> Result<T, Self::Error>
    where
        T: DeserializeOwned,
    {
        postcard::from_bytes(slice)
    }
}
