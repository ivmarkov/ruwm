use core::future::Future;

extern crate alloc;
use alloc::boxed::Box;

use embedded_svc::unblocker::asyncs::{Blocker, Unblocker};

#[derive(Clone)]
pub struct SmolBlocker;

impl Blocker<'static> for SmolBlocker {
    fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        smol::block_on(f)
    }
}

#[derive(Clone)]
pub struct SmolUnblocker;

impl Unblocker for SmolUnblocker {
    type UnblockFuture<T> = impl Future<Output = T>;

    fn unblock<F, T>(&self, f: F) -> Self::UnblockFuture<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // TODO: Need to box or else we get rustc error:
        // "type parameter `F` is part of concrete type but not used in parameter list for the `impl Trait` type alias"
        let boxed: Box<dyn FnOnce() -> T + Send + 'static> = Box::new(f);
        smol::unblock(boxed)
    }
}
