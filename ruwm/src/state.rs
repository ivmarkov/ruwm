use core::marker::PhantomData;

use serde::{de::DeserializeOwned, Serialize};

use embedded_svc::channel::asynch::Sender;
use embedded_svc::mutex::RawMutex;
use embedded_svc::storage::{SerDe, Storage};
use embedded_svc::utils::mutex::Mutex;

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

pub struct MemoryStateCell<R, T>(Mutex<R, T>)
where
    R: RawMutex;

impl<R, T> MemoryStateCell<R, T>
where
    R: RawMutex,
{
    pub fn new(data: T) -> Self {
        Self(Mutex::new(data))
    }
}

impl<R, T> StateCellRead for MemoryStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    type Data = T;

    fn get(&self) -> Self::Data {
        self.0.lock().clone()
    }
}

impl<R, T> StateCell for MemoryStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let mut guard = self.0.lock();

        let old_data = (&*guard).clone();

        *guard = data;

        old_data
    }
}

pub struct MutRefStateCell<R, T: 'static>(Mutex<R, &'static mut T>);

impl<R, T> MutRefStateCell<R, T>
where
    R: RawMutex,
    T: 'static,
{
    pub fn new(data: &'static mut T) -> Self {
        Self(Mutex::new(data))
    }
}

impl<R, T> StateCellRead for MutRefStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    type Data = T;

    fn get(&self) -> Self::Data {
        self.0.lock().clone()
    }
}

impl<R, T> StateCell for MutRefStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let mut guard = self.0.lock();

        let old_data = (&**guard).clone();

        **guard = data;

        old_data
    }
}

pub struct CachingStateCell<R, C, S>(Mutex<R, (C, S)>);

impl<R, C, S> CachingStateCell<R, C, S>
where
    R: RawMutex,
{
    pub fn new(cache: C, state: S) -> Self {
        Self(Mutex::new((cache, state)))
    }
}

impl<R, C, S> StateCellRead for CachingStateCell<R, C, S>
where
    R: RawMutex,
    C: StateCell<Data = Option<S::Data>>,
    S: StateCell,
    S::Data: Clone,
{
    type Data = S::Data;

    fn get(&self) -> Self::Data {
        let guard = self.0.lock();

        if let Some(data) = guard.0.get() {
            data
        } else {
            let data = guard.1.get();

            guard.0.set(Some(data.clone()));

            data
        }
    }
}

impl<R, C, S> StateCell for CachingStateCell<R, C, S>
where
    R: RawMutex,
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

pub struct WearLevelingStateCell<const N: usize, R, C>(Mutex<R, (C, usize)>);

impl<const N: usize, R, C> WearLevelingStateCell<N, R, C>
where
    R: RawMutex,
{
    pub fn new(state: C) -> Self {
        Self(Mutex::new((state, 0)))
    }
}

impl<const N: usize, R, C> StateCellRead for WearLevelingStateCell<N, R, C>
where
    R: RawMutex,
    C: StateCell,
{
    type Data = C::Data;

    fn get(&self) -> Self::Data {
        self.0.lock().0.get()
    }
}

impl<const N: usize, R, C> StateCell for WearLevelingStateCell<N, R, C>
where
    R: RawMutex,
    C: StateCell,
    C::Data: PartialEq,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let mut guard = self.0.lock();

        let old_data = guard.0.get();
        if old_data != data {
            if guard.1 >= N {
                guard.1 = 0;

                guard.0.set(data);
            } else {
                guard.1 += 1;
            }
        }

        old_data
    }
}

pub struct StorageStateCell<'a, R, S, T>
where
    R: 'a,
    S: 'a,
{
    storage: &'a Mutex<R, S>,
    name: &'a str,
    _data: PhantomData<fn() -> T>,
}

impl<'a, R, S, T> StorageStateCell<'a, R, S, T> {
    pub fn new(storage: &'a Mutex<R, S>, name: &'a str) -> Self {
        Self {
            storage,
            name,
            _data: PhantomData,
        }
    }
}

impl<'a, R, S, T> StateCellRead for StorageStateCell<'a, R, S, T>
where
    R: RawMutex + 'a,
    S: Storage + 'a,
    T: Serialize + DeserializeOwned + 'static,
{
    type Data = T;

    fn get(&self) -> Self::Data {
        self.storage.lock().get(self.name).unwrap().unwrap()
    }
}

impl<'a, R, S, T> StateCell for StorageStateCell<'a, R, S, T>
where
    R: RawMutex + 'a,
    S: Storage + 'a,
    T: Serialize + DeserializeOwned + 'static,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        let mut guard = self.storage.lock();

        let old_data = guard.get(self.name).unwrap().unwrap();

        guard.set(self.name, &data).unwrap();

        old_data
    }
}

pub type PostcardStorageStateCell<'a, const N: usize, R, S, T> =
    StorageStateCell<'a, R, PostcardStorage<N, S>, T>;

pub type PostcardStorage<const N: usize, S> =
    embedded_svc::storage::StorageImpl<N, S, PostcardSerDe>;

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
