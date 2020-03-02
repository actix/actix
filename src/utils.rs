use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_channel::oneshot;

use crate::actor::Actor;
use crate::clock::{interval_at, Delay, Instant, Interval};
use crate::fut::{ActorFuture, ActorStream};

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

/// An `ActorFuture` that runs a function in the actor's context after a specified amount of time.
///
/// Unless you specifically need access to the future, use [`Context::run_later`] instead.
///
/// [`Context::run_later`]: ../prelude/trait.AsyncContext.html#method.run_later
///
/// ```rust
/// # use std::io;
/// use std::time::Duration;
/// use actix::prelude::*;
/// use actix::utils::TimerFunc;
///
/// struct MyActor;
///
/// impl MyActor {
///     fn stop(&mut self, context: &mut Context<Self>) {
///         System::current().stop();
///     }
/// }
///
/// impl Actor for MyActor {
///    type Context = Context<Self>;
///
///    fn started(&mut self, context: &mut Context<Self>) {
///        // spawn a delayed future into our context
///        TimerFunc::new(Duration::from_millis(100), Self::stop)
///            .spawn(context);
///    }
/// }
/// # fn main() {
/// #    let sys = System::new("example");
/// #    let addr = MyActor.start();
/// #    sys.run();
/// # }

#[must_use = "future do nothing unless polled"]
pub struct TimerFunc<A>
where
    A: Actor,
{
    f: Option<Box<dyn TimerFuncBox<A>>>,
    timeout: Delay,
}

impl<A> TimerFunc<A>
where
    A: Actor,
{
    /// Creates a new `TimerFunc` with the given duration.
    pub fn new<F>(timeout: Duration, f: F) -> TimerFunc<A>
    where
        F: FnOnce(&mut A, &mut A::Context) + 'static,
    {
        TimerFunc {
            f: Some(Box::new(f)),
            timeout: tokio::time::delay_for(timeout),
        }
    }
}

trait TimerFuncBox<A: Actor>: 'static {
    fn call(self: Box<Self>, _: &mut A, _: &mut A::Context);
}

impl<A: Actor, F: FnOnce(&mut A, &mut A::Context) + 'static> TimerFuncBox<A> for F {
    #[allow(clippy::boxed_local)]
    fn call(self: Box<Self>, act: &mut A, ctx: &mut A::Context) {
        (*self)(act, ctx)
    }
}

impl<A> ActorFuture for TimerFunc<A>
where
    A: Actor,
{
    type Output = ();
    type Actor = A;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.get_mut();
        match Pin::new(&mut this.timeout).poll(task) {
            Poll::Ready(_) => {
                if let Some(f) = this.f.take() {
                    f.call(act, ctx);
                }
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// An `ActorStream` that periodically runs a function in the actor's context.
///
/// Unless you specifically need access to the future, use [`Context::run_interval`] instead.
///
/// [`Context::run_interval`]: ../prelude/trait.AsyncContext.html#method.run_interval
///
/// ```rust
/// # use std::io;
/// use std::time::Duration;
/// use actix::prelude::*;
/// use actix::utils::IntervalFunc;
///
/// struct MyActor;
///
/// impl MyActor {
///     fn tick(&mut self, context: &mut Context<Self>) {
///         println!("tick");
///     }
/// }
///
/// impl Actor for MyActor {
///    type Context = Context<Self>;
///
///    fn started(&mut self, context: &mut Context<Self>) {
///        // spawn an interval stream into our context
///        IntervalFunc::new(Duration::from_millis(100), Self::tick)
///            .finish()
///            .spawn(context);
/// #      context.run_later(Duration::from_millis(200), |_, _| System::current().stop());
///    }
/// }
/// # fn main() {
/// #    let sys = System::new("example");
/// #    let addr = MyActor.start();
/// #    sys.run();
/// # }
/// ```
#[must_use = "future do nothing unless polled"]
pub struct IntervalFunc<A: Actor> {
    f: Box<dyn IntervalFuncBox<A>>,
    interval: Interval,
}

impl<A: Actor> IntervalFunc<A> {
    /// Creates a new `IntervalFunc` with the given interval duration.
    pub fn new<F>(timeout: Duration, f: F) -> IntervalFunc<A>
    where
        F: FnMut(&mut A, &mut A::Context) + 'static,
    {
        Self {
            f: Box::new(f),
            interval: interval_at(Instant::now() + timeout, timeout),
        }
    }
}

trait IntervalFuncBox<A: Actor>: 'static {
    fn call(&mut self, _: &mut A, _: &mut A::Context);
}

impl<A: Actor, F: FnMut(&mut A, &mut A::Context) + 'static> IntervalFuncBox<A> for F {
    #[allow(clippy::boxed_local)]
    fn call(&mut self, act: &mut A, ctx: &mut A::Context) {
        (*self)(act, ctx)
    }
}

impl<A: Actor> ActorStream for IntervalFunc<A> {
    type Item = ();
    type Actor = A;

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            match this.interval.poll_tick(task) {
                Poll::Ready(_) => {
                    //Interval Stream cannot return None
                    this.f.call(act, ctx);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
