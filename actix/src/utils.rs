use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_core::ready;
use pin_project_lite::pin_project;
use tokio::sync::oneshot;

use crate::actor::Actor;
use crate::clock::{sleep, Sleep};
use crate::fut::{ActorFuture, ActorStream};

#[deprecated(
    since = "0.11.0",
    note = "Please use tokio::sync::oneshot::Sender instead."
)]
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

pin_project! {
    /// An `ActorFuture` that runs a function in the actor's context after a specified amount of time.
    ///
    /// Unless you specifically need access to the future, use [`Context::run_later`] instead.
    ///
    /// [`Context::run_later`]: ../prelude/trait.AsyncContext.html#method.run_later
    ///
    /// ```
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
    /// #    let mut sys = System::new();
    /// #    let addr = sys.block_on(async { MyActor.start() });
    /// #    sys.run();
    /// # }
    #[must_use = "future do nothing unless polled"]
    #[allow(clippy::type_complexity)]
    pub struct TimerFunc<A: Actor> {
        f: Option<Box<dyn FnOnce(&mut A, &mut A::Context)>>,
        #[pin]
        timeout: Sleep,
    }
}

impl<A: Actor> TimerFunc<A> {
    /// Creates a new `TimerFunc` with the given duration.
    pub fn new<F>(timeout: Duration, f: F) -> TimerFunc<A>
    where
        F: FnOnce(&mut A, &mut A::Context) + 'static,
    {
        TimerFunc {
            f: Some(Box::new(f)),
            timeout: sleep(timeout),
        }
    }
}

impl<A> ActorFuture<A> for TimerFunc<A>
where
    A: Actor,
{
    type Output = ();

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.project();
        ready!(this.timeout.poll(task));
        let f = this.f.take().expect("TimerFunc polled after finish");
        f(act, ctx);
        Poll::Ready(())
    }
}

pin_project! {
    /// An `ActorStream` that periodically runs a function in the actor's context.
    ///
    /// Unless you specifically need access to the future, use [`Context::run_interval`] instead.
    ///
    /// [`Context::run_interval`]: ../prelude/trait.AsyncContext.html#method.run_interval
    ///
    /// ```
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
    /// #    let mut sys = System::new();
    /// #    let addr = sys.block_on(async { MyActor.start() });
    /// #    sys.run();
    /// # }
    /// ```
    #[must_use = "future do nothing unless polled"]
    pub struct IntervalFunc<A: Actor> {
        f: Box<dyn FnMut(&mut A, &mut A::Context)>,
        dur: Duration,
        #[pin]
        timer: Sleep,
    }
}

impl<A: Actor> IntervalFunc<A> {
    /// Creates a new `IntervalFunc` with the given interval duration.
    pub fn new<F>(dur: Duration, f: F) -> IntervalFunc<A>
    where
        F: FnMut(&mut A, &mut A::Context) + 'static,
    {
        Self {
            f: Box::new(f),
            dur,
            timer: sleep(dur),
        }
    }
}

impl<A: Actor> ActorStream<A> for IntervalFunc<A> {
    type Item = ();

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            ready!(this.timer.as_mut().poll(task));
            let now = this.timer.deadline();
            this.timer.as_mut().reset(now + *this.dur);
            (this.f)(act, ctx);
        }
    }
}
