use std::time::{Duration, Instant};

use futures::unsync::oneshot;
use futures::{Async, Future, Stream, Poll};
use tokio_timer::{Delay, Interval};

use actor::Actor;
use fut::{ActorFuture, ActorStream};

pub struct Condition<T>
where
    T: Clone,
{
    waiters: Vec<oneshot::Sender<T>>,
}

impl<T> Condition<T>
where
    T: Clone,
{
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

impl<T> Default for Condition<T>
where
    T: Clone,
{
    fn default() -> Self {
        Condition {
            waiters: Vec::new(),
        }
    }
}

pub(crate) struct TimerFunc<A>
where
    A: Actor,
{
    f: Option<Box<TimerFuncBox<A>>>,
    timeout: Delay,
}

impl<A> TimerFunc<A>
where
    A: Actor,
{
    pub fn new<F>(timeout: Duration, f: F) -> TimerFunc<A>
    where
        F: FnOnce(&mut A, &mut A::Context) + 'static,
    {
        TimerFunc {
            f: Some(Box::new(f)),
            timeout: Delay::new(Instant::now() + timeout),
        }
    }
}

trait TimerFuncBox<A: Actor>: 'static {
    fn call(self: Box<Self>, &mut A, &mut A::Context);
}

impl<A: Actor, F: FnOnce(&mut A, &mut A::Context) + 'static> TimerFuncBox<A> for F {
    #[cfg_attr(feature = "cargo-clippy", allow(boxed_local))]
    fn call(self: Box<Self>, act: &mut A, ctx: &mut A::Context) {
        (*self)(act, ctx)
    }
}

#[doc(hidden)]
impl<A> ActorFuture for TimerFunc<A>
where
    A: Actor,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(
        &mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context,
    ) -> Poll<Self::Item, Self::Error> {
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

pub(crate) struct IntervalFunc<A: Actor> {
    f: Box<IntervalFuncBox<A>>,
    interval: Interval,
}

impl<A: Actor> IntervalFunc<A> {
    pub fn new<F>(timeout: Duration, f: F) -> IntervalFunc<A>
    where
        F: FnMut(&mut A, &mut A::Context) + 'static,
    {
        Self {
            f: Box::new(f),
            interval: Interval::new(Instant::now(), timeout),
        }
    }
}

trait IntervalFuncBox<A: Actor>: 'static {
    fn call(&mut self, &mut A, &mut A::Context);
}

impl<A: Actor, F: FnMut(&mut A, &mut A::Context) + 'static> IntervalFuncBox<A> for F {
    #[cfg_attr(feature = "cargo-clippy", allow(boxed_local))]
    fn call(&mut self, act: &mut A, ctx: &mut A::Context) {
        self(act, ctx)
    }
}

#[doc(hidden)]
impl<A: Actor> ActorStream for IntervalFunc<A> {
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut Self::Actor, ctx: &mut <Self::Actor as Actor>::Context,
    ) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            match self.interval.poll() {
                Ok(Async::Ready(_)) => {
                    //Interval Stream cannot return None
                    self.f.call(act, ctx);
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(_) => unreachable!(),
            }
        }
    }
}

