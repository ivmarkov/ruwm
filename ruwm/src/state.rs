use core::cell::RefCell;
use core::marker::PhantomData;

use embassy_util::blocking_mutex::raw::RawMutex;
use embassy_util::blocking_mutex::Mutex;
use serde::{de::DeserializeOwned, Serialize};

use embedded_svc::channel::asynch::Sender;
use embedded_svc::storage::{SerDe, Storage};

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

pub struct NoopStateCell;

impl StateCellRead for NoopStateCell {
    type Data = ();

    fn get(&self) {}
}

pub struct MemoryStateCell<R, T>(Mutex<R, RefCell<T>>)
where
    R: RawMutex;

impl<R, T> MemoryStateCell<R, T>
where
    R: RawMutex,
{
    pub const fn new(data: T) -> Self {
        Self(Mutex::new(RefCell::new(data)))
    }
}

impl<R, T> StateCellRead for MemoryStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    type Data = T;

    fn get(&self) -> Self::Data {
        self.0.lock(|state| *state.borrow())
    }
}

impl<R, T> StateCell for MemoryStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        self.0.lock(|state| state.replace(data))
    }
}

pub struct MutRefStateCell<R, T: 'static>(Mutex<R, &'static mut T>);

impl<R, T> MutRefStateCell<R, T>
where
    R: RawMutex,
    T: 'static,
{
    pub const fn new(data: &'static mut T) -> Self {
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
        self.0.lock(|state| **state)
    }
}

impl<R, T> StateCell for MutRefStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        self.0.lock(|state| {
            let old = **state.clone();
            **state = data;

            old
        })
    }
}

pub struct CachingStateCell<R, C, S>(Mutex<R, (C, S)>);

impl<R, C, S> CachingStateCell<R, C, S>
where
    R: RawMutex,
{
    pub const fn new(cache: C, state: S) -> Self {
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
        self.0.lock(|state| {
            if let Some(data) = state.0.get() {
                data
            } else {
                let data = state.1.get();

                state.0.set(Some(data));

                data
            }
        })
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
        self.0.lock(|state| {
            let old_data = state.0.get();

            if let Some(old_data) = old_data {
                if old_data != data {
                    state.0.set(Some(data.clone()));
                    state.1.set(data);
                }

                old_data
            } else {
                state.0.set(Some(data.clone()));

                state.1.set(data)
            }
        })
    }
}

pub struct WearLevelingStateCell<const N: usize, R, C>(Mutex<R, (C, usize)>);

impl<const N: usize, R, C> WearLevelingStateCell<N, R, C>
where
    R: RawMutex,
{
    pub const fn new(state: C) -> Self {
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
        self.0.lock(|state| state.0.get())
    }
}

impl<const N: usize, R, C> StateCell for WearLevelingStateCell<N, R, C>
where
    R: RawMutex,
    C: StateCell,
    C::Data: PartialEq,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        self.0.lock(|state| {
            let old_data = state.0.get();
            if old_data != data {
                if state.1 >= N {
                    state.1 = 0;

                    state.0.set(data);
                } else {
                    state.1 += 1;
                }
            }

            old_data
        })
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
    pub const fn new(storage: &'a Mutex<R, S>, name: &'a str) -> Self {
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
        self.storage
            .lock(|state| state.get(self.name).unwrap().unwrap())
    }
}

impl<'a, R, S, T> StateCell for StorageStateCell<'a, R, S, T>
where
    R: RawMutex + 'a,
    S: Storage + 'a,
    T: Serialize + DeserializeOwned + 'static,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        self.storage.lock(|state| {
            let old_data = state.get(self.name).unwrap().unwrap();

            state.set(self.name, &data).unwrap();

            old_data
        })
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
