use core::future::Future;

use embedded_svc::unblocker::nonblocking::{Blocker, Unblocker};

pub struct SmolBlocker;

impl Blocker for SmolBlocker {
    fn block<F>(f: F) -> F::Output
    where
        F: Future,
    {
        smol::block_on(f)
    }
}

pub struct SmolUnblocker;

// TODO: Need to change the Unblocker trait to take self
// pub fn unblocker() -> impl Unblocker {
//     // env::set_var("BLOCKING_MAX_THREADS", "2");

//     SmolUnblocker
// }

impl Unblocker for SmolUnblocker {
    type UnblockFuture<T> = impl Future<Output = T>;

    fn unblock<T>(f: Box<dyn FnOnce() -> T + Send + 'static>) -> Self::UnblockFuture<T>
    where
        T: Send + 'static,
    {
        smol::unblock(f)
    }
}
