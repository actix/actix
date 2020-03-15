use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::Stream;
use log::error;
use pin_project::pin_project;

use crate::actor::{Actor, ActorContext, ActorState, AsyncContext, SpawnHandle};
use crate::fut::ActorFuture;

/// Stream handler
///
/// This is helper trait that allows to handle `Stream` in
/// a similar way as normal actor messages.
/// When stream resolves to a next item, `handle()` method of this trait
/// get called. If stream produces error, `error()` method get called.
/// Depends on result of the `error()` method, actor could continue to
/// process stream items or stop stream processing.
/// When stream completes, `finished()` method get called. By default
/// `finished()` method stops actor execution.
#[allow(unused_variables)]
pub trait StreamHandler<I>
where
    Self: Actor,
{
    /// Method is called for every message received by this Actor
    fn handle(&mut self, item: I, ctx: &mut Self::Context);

    /// Method is called when stream get polled first time.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Method is called when stream finishes.
    ///
    /// By default this method stops actor execution.
    fn finished(&mut self, ctx: &mut Self::Context) {
        ctx.stop()
    }

    /// This method register stream to an actor context and
    /// allows to handle `Stream` in similar way as normal actor messages.
    ///
    /// ```rust
    /// use actix::prelude::*;
    /// use futures_util::stream::once;
    ///
    /// #[derive(Message)]
    /// #[rtype(result = "()")]
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl StreamHandler<Ping> for MyActor {
    ///
    ///     fn handle(&mut self, item: Ping, ctx: &mut Context<MyActor>) {
    ///         println!("PING");
    ///         System::current().stop()
    ///     }
    ///
    ///     fn finished(&mut self, ctx: &mut Self::Context) {
    ///         println!("finished");
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///    type Context = Context<Self>;
    ///
    ///    fn started(&mut self, ctx: &mut Context<Self>) {
    ///        // add stream
    ///        Self::add_stream(once(async { Ping }), ctx);
    ///    }
    /// }
    ///
    /// fn main() {
    ///     let sys = System::new("example");
    ///     let addr = MyActor.start();
    ///     sys.run();
    /// }
    /// ```
    fn add_stream<S>(fut: S, ctx: &mut Self::Context) -> SpawnHandle
    where
        Self::Context: AsyncContext<Self>,
        S: Stream<Item = I> + 'static,
        I: 'static,
    {
        if ctx.state() == ActorState::Stopped {
            error!("Context::add_stream called for stopped actor.");
            SpawnHandle::default()
        } else {
            ctx.spawn(ActorStream::new(fut))
        }
    }
}

#[pin_project]
pub(crate) struct ActorStream<A, M, S> {
    #[pin]
    stream: S,
    started: bool,
    act: PhantomData<A>,
    msg: PhantomData<M>,
}

impl<A, M, S> ActorStream<A, M, S> {
    pub fn new(fut: S) -> Self {
        Self {
            stream: fut,
            started: false,
            act: PhantomData,
            msg: PhantomData,
        }
    }
}

impl<A, M, S> ActorFuture for ActorStream<A, M, S>
where
    S: Stream<Item = M>,
    A: Actor + StreamHandler<M>,
    A::Context: AsyncContext<A>,
{
    type Output = ();
    type Actor = A;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let mut this = self.project();

        if !*this.started {
            *this.started = true;
            <A as StreamHandler<M>>::started(act, ctx);
        }

        match this.stream.as_mut().poll_next(task) {
            Poll::Ready(Some(msg)) => {
                A::handle(act, msg, ctx);
                if !ctx.waiting() {
                    // Let the future's context know that this future might be polled right the way
                    task.waker().wake_by_ref();
                }
                Poll::Pending
            }
            Poll::Ready(None) => {
                A::finished(act, ctx);
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
