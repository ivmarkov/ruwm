//! A synchronization primitive for passing the latest value to a task.
use core::cell::RefCell;
use core::future::Future;
use core::mem;
use core::task::{Context, Poll, Waker};

use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;

/// Single-slot signaling primitive.
///
/// This is similar to a [`Channel`](crate::channel::mpmc::Channel) with a buffer size of 1, except
/// "sending" to it (calling [`Signal::signal`]) when full will overwrite the previous value instead
/// of waiting for the receiver to pop the previous value.
///
/// It is useful for sending data between tasks when the receiver only cares about
/// the latest data, and therefore it's fine to "lose" messages. This is often the case for "state"
/// updates.
///
/// For more advanced use cases, you might want to use [`Channel`](crate::channel::mpmc::Channel) instead.
///
/// Signals are generally declared as `static`s and then borrowed as required.
///
/// ```
/// use embassy_util::channel::signal::Signal;
///
/// enum SomeCommand {
///   On,
///   Off,
/// }
///
/// static SOME_SIGNAL: Signal<SomeCommand> = Signal::new();
/// ```
pub struct Signal<R, T>
where
    R: RawMutex,
{
    state: Mutex<R, RefCell<State<T>>>,
}

enum State<T> {
    None,
    Waiting(Waker),
    Signaled(T),
}

impl<R, T> Signal<R, T>
where
    R: RawMutex,
{
    /// Create a new `Signal`.
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(RefCell::new(State::None)),
        }
    }
}

impl<R, T: Send> Signal<R, T>
where
    R: RawMutex,
{
    /// Mark this Signal as signaled.
    pub fn signal(&self, val: T) {
        self.state.lock(|state| {
            let state = state.replace(State::Signaled(val));
            if let State::Waiting(waker) = state {
                waker.wake();
            }
        })
    }

    /// Remove the queued value in this `Signal`, if any.
    pub fn reset(&self) {
        self.state.lock(|state| {
            *state.borrow_mut() = State::None;
        })
    }

    fn poll_wait(&self, cx: &mut Context<'_>) -> Poll<T> {
        self.state.lock(|state| {
            let mut state = state.borrow_mut();
            match &mut *state {
                State::None => {
                    *state = State::Waiting(cx.waker().clone());
                    Poll::Pending
                }
                State::Waiting(w) if w.will_wake(cx.waker()) => Poll::Pending,
                State::Waiting(_) => panic!("waker overflow"),
                State::Signaled(_) => match mem::replace(&mut *state, State::None) {
                    State::Signaled(res) => Poll::Ready(res),
                    _ => unreachable!(),
                },
            }
        })
    }

    /// Future that completes when this Signal has been signaled.
    pub fn wait(&self) -> impl Future<Output = T> + '_ {
        futures::future::poll_fn(move |cx| self.poll_wait(cx))
    }

    /// non-blocking method to check whether this signal has been signaled.
    pub fn signaled(&self) -> bool {
        self.state
            .lock(|state| matches!(*state.borrow(), State::Signaled(_)))
    }
}
