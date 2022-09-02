use core::cell::{Cell, RefCell};
use core::fmt::Debug;
use core::marker::PhantomData;

use serde::{de::DeserializeOwned, Serialize};

use log::info;

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;

use embedded_svc::storage::{SerDe, Storage};

use crate::channel::Sender;

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

pub async fn update_with<'a, S, D>(
    state_name: &'static str,
    state: &'a S,
    updater: impl FnOnce(D) -> D + 'a,
    notif: &'a mut impl Sender<Data = ()>,
) -> bool
where
    S: StateCell<Data = D>,
    D: PartialEq + Debug,
{
    let (old, new) = state.update(updater);

    if old != new {
        info!("{} STATE: {:?}", state_name, new);

        notif.send(());

        true
    } else {
        false
    }
}

pub async fn update<S, D>(
    state_name: &'static str,
    state: &S,
    data: D,
    notif: &mut impl Sender<Data = ()>,
) -> bool
where
    S: StateCell<Data = D>,
    D: PartialEq + Debug,
{
    update_with(state_name, state, move |_| data, notif).await
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
        self.0.lock(|state| state.borrow().clone())
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

pub struct MutRefStateCell<R, T: 'static>(Mutex<R, RefCell<&'static mut T>>);

impl<R, T> MutRefStateCell<R, T>
where
    R: RawMutex,
    T: 'static,
{
    pub fn new(data: &'static mut T) -> Self {
        Self(Mutex::new(RefCell::new(data)))
    }
}

impl<R, T> StateCellRead for MutRefStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    type Data = T;

    fn get(&self) -> Self::Data {
        self.0.lock(|state| (**state.borrow()).clone())
    }
}

impl<R, T> StateCell for MutRefStateCell<R, T>
where
    R: RawMutex,
    T: Clone,
{
    fn set(&self, data: Self::Data) -> Self::Data {
        self.0.lock(|state| {
            let old = (**state.borrow()).clone();
            **state.borrow_mut() = data;

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

                state.0.set(Some(data.clone()));

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

pub struct WearLevelingStateCell<const N: usize, R, C>(Mutex<R, (C, Cell<usize>)>);

impl<const N: usize, R, C> WearLevelingStateCell<N, R, C>
where
    R: RawMutex,
{
    pub fn new(state: C) -> Self {
        Self(Mutex::new((state, Cell::new(0))))
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
                if state.1.get() >= N {
                    state.1.set(0);
                    state.0.set(data);
                } else {
                    state.1.set(state.1.get() + 1);
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
    storage: &'a Mutex<R, RefCell<S>>,
    name: &'a str,
    _data: PhantomData<fn() -> T>,
}

impl<'a, R, S, T> StorageStateCell<'a, R, S, T> {
    pub const fn new(storage: &'a Mutex<R, RefCell<S>>, name: &'a str) -> Self {
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
            .lock(|state| state.borrow().get(self.name).unwrap().unwrap())
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
            let old_data = state.borrow().get(self.name).unwrap().unwrap();

            state.borrow_mut().set(self.name, &data).unwrap();

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
