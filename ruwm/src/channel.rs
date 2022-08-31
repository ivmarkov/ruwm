use core::future::Future;

pub trait Sender {
    type Data: Send;

    type SendFuture<'a>: Future
    where
        Self: 'a;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_>;
}

impl<S> Sender for &mut S
where
    S: Sender,
{
    type Data = S::Data;

    type SendFuture<'a>
    = S::SendFuture<'a> where Self: 'a;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        (*self).send(value)
    }
}

pub trait Receiver {
    type Data: Send;

    type RecvFuture<'a>: Future<Output = Self::Data>
    where
        Self: 'a;

    fn recv(&mut self) -> Self::RecvFuture<'_>;
}

impl<R> Receiver for &mut R
where
    R: Receiver,
{
    type Data = R::Data;

    type RecvFuture<'a>
    = R::RecvFuture<'a> where Self: 'a;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        (*self).recv()
    }
}
