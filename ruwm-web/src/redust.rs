use std::{ops::Deref, ptr, rc::Rc};

use yew::{use_context, Reducible, UseReducerHandle};

pub struct Selector<T, R, A>
where
    T: Reducible,
{
    reducible_selector: Rc<dyn Fn(&T) -> &R>,
    dispatch_selector: Rc<dyn Fn(A) -> T::Action>,
}

impl<T, R, A> Selector<T, R, A>
where
    T: Reducible,
{
    pub fn new(
        reducible_selector: impl Fn(&T) -> &R + 'static,
        dispatch_selector: impl Fn(A) -> T::Action + 'static,
    ) -> Self {
        Self {
            reducible_selector: Rc::new(reducible_selector),
            dispatch_selector: Rc::new(dispatch_selector),
        }
    }
}

impl<T, R, A> Clone for Selector<T, R, A>
where
    T: Reducible,
{
    fn clone(&self) -> Self {
        Self {
            reducible_selector: self.reducible_selector.clone(),
            dispatch_selector: self.dispatch_selector.clone(),
        }
    }
}

impl<T, R, A> PartialEq for Selector<T, R, A>
where
    T: Reducible,
{
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(
            &*self.reducible_selector as *const _,
            &*other.reducible_selector as *const _,
        ) && ptr::eq(
            &*self.dispatch_selector as *const _,
            &*other.dispatch_selector as *const _,
        )
    }
}

pub struct UseSelectorHandle<T, R, A>
where
    T: Reducible,
{
    selector: Selector<T, R, A>,
    reducer_handle: UseReducerHandle<T>,
}

impl<T, R, A> UseSelectorHandle<T, R, A>
where
    T: Reducible,
{
    pub fn dispatch(&self, action: A) {
        self.reducer_handle
            .dispatch((self.selector.dispatch_selector)(action))
    }
}

impl<T, R, A> Deref for UseSelectorHandle<T, R, A>
where
    T: Reducible,
{
    type Target = R;

    fn deref(&self) -> &Self::Target {
        (self.selector.reducible_selector)(&*self.reducer_handle)
    }
}

pub fn use_selector<T: Store, R, A>(selector: Selector<T, R, A>) -> UseSelectorHandle<T, R, A> {
    let store = use_context::<StoreContext<T>>().expect("No Store context found");

    UseSelectorHandle {
        selector,
        reducer_handle: store.0,
    }
}

pub trait Store: Reducible + Clone + PartialEq + 'static
/*where <Self as Reducible>::Action: PartialEq + 'static*/
{
}

impl<T> Store for T
where
    T: Reducible + Clone + PartialEq + 'static,
    T::Action: PartialEq + 'static,
{
}

pub struct StoreContext<T: Store>(UseReducerHandle<T>);

impl<T: Store> StoreContext<T> {
    pub fn new(reducer_handle: UseReducerHandle<T>) -> Self {
        Self(reducer_handle)
    }
}

impl<T: Store> Clone for StoreContext<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Store> PartialEq for StoreContext<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[derive(Clone, PartialEq)]
pub struct SimpleStore<S>(S);

impl<S> SimpleStore<S> {
    pub fn new(state: S) -> Self {
        Self(state)
    }
}

impl<S> Reducible for SimpleStore<S> {
    type Action = SimpleStoreAction<S>;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        let this = match action {
            Self::Action::Update(state) => Self(state),
        };

        this.into()
    }
}

impl<S> Deref for SimpleStore<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(PartialEq)]
pub enum SimpleStoreAction<S> {
    Update(S),
}
