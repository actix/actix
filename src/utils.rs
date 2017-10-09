use std::time::Duration;
use std::marker::PhantomData;
use futures::{Async, Future, Poll};
use futures::unsync::oneshot;
use tokio_core::reactor::Timeout;

use fut::ActorFuture;
use actor::Actor;
use arbiter::Arbiter;


#[doc(hidden)]
pub struct Condition<T> where T: Clone {
    waiters: Vec<oneshot::Sender<T>>,
}

impl<T> Condition<T> where T: Clone {

    pub fn wait(&mut self) -> oneshot::Receiver<T> {
        let (tx, rx) = oneshot::channel();
        self.waiters.push(tx);
        rx
    }

    pub fn set(self, result: T) {
        for waiter in self.waiters {
            let _ = waiter.send(result.clone());
        }
    }
}

impl<T> Default for Condition<T> where T: Clone {
    fn default() -> Self {
        Condition { waiters: Vec::new() }
    }
}

pub(crate) struct TimeoutWrapper<M, E> {
    msg: Option<M>,
    err: PhantomData<E>,
    timeout: Timeout,
}

impl<M, E> TimeoutWrapper<M, E> {
    pub fn new(msg: M, timeout: Duration) -> TimeoutWrapper<M, E> {
        TimeoutWrapper{
            msg: Some(msg),
            err: PhantomData,
            timeout: Timeout::new(timeout, Arbiter::handle()).unwrap()}
    }
}


#[doc(hidden)]
impl<M, E> Future for TimeoutWrapper<M, E>
{
    type Item = M;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.timeout.poll() {
            Ok(Async::Ready(_)) => Ok(Async::Ready(self.msg.take().unwrap())),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(_) => unreachable!(),
        }
    }
}

pub(crate)
struct TimerFunc<A> where A: Actor {
    f: Option<Box<TimerFuncBox<A>>>,
    timeout: Timeout,
}

impl<A> TimerFunc<A> where A: Actor {
    pub fn new<F>(timeout: Duration, f: F) -> TimerFunc<A>
        where F: FnOnce(& mut A, & mut A::Context) + 'static
    {
        TimerFunc {
            f: Some(Box::new(f)),
            timeout: Timeout::new(timeout, Arbiter::handle()).unwrap()}
    }
}

trait TimerFuncBox<A: Actor>: 'static {
    fn call(self: Box<Self>, &mut A, &mut A::Context);
}

impl<A: Actor, F: FnOnce(&mut A, &mut A::Context) + 'static> TimerFuncBox<A> for F {
    #[cfg_attr(feature="cargo-clippy", allow(boxed_local))]
    fn call(self: Box<Self>, act: &mut A, ctx: &mut A::Context) {
        (*self)(act, ctx)
    }
}


#[doc(hidden)]
impl<A> ActorFuture for TimerFunc<A> where A: Actor {
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context)
            -> Poll<Self::Item, Self::Error>
    {
        match self.timeout.poll() {
            Ok(Async::Ready(_)) => {
                if let Some(f) = self.f.take() {
                    f.call(act, ctx);
                }
                Ok(Async::Ready(()))
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(_) => unreachable!(),
        }
    }
}
