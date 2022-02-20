use core::future::Future;

use embedded_svc::unblocker::nonblocking::Unblocker;

pub struct SmolUnblocker;

impl Unblocker for SmolUnblocker {
    type UnblockFuture<T> = impl Future<Output = T>;

    fn unblock<T>(f: Box<dyn FnOnce() -> T + Send + 'static>) -> Self::UnblockFuture<T>
    where
        T: Send + 'static,
    {
        smol::unblock(f)
    }
}
