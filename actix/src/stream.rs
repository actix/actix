use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::{ready, stream::Stream};
use log::error;
use pin_project_lite::pin_project;

use crate::actor::{Actor, ActorContext, ActorState, AsyncContext, SpawnHandle};
use crate::fut::ActorFuture;

/// Stream handling for Actors.
///
/// This is helper trait that allows handling [`Stream`]s in a similar way to normal actor messages.
/// When stream resolves its next item, `handle()` is called with that item.
///
/// When the stream completes, `finished()` is called. By default, it stops Actor execution.
///
/// # Examples
/// ```
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
///        Self::add_stream(once(async { Ping }), ctx);
///    }
/// }
///
/// fn main() {
///     let mut sys = System::new();
///     let addr = sys.block_on(async { MyActor.start() });
///     sys.run();
/// }
/// ```
#[allow(unused_variables)]
pub trait StreamHandler<I>
where
    Self: Actor,
{
    /// Called for every message emitted by the stream.
    fn handle(&mut self, item: I, ctx: &mut Self::Context);

    /// Called when stream emits first item.
    ///
    /// Default implementation does nothing.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Called when stream finishes.
    ///
    /// Default implementation stops Actor execution.
    fn finished(&mut self, ctx: &mut Self::Context) {
        ctx.stop()
    }

    /// Register a Stream to the actor context.
    fn add_stream<S>(stream: S, ctx: &mut Self::Context) -> SpawnHandle
    where
        S: Stream + 'static,
        Self: StreamHandler<S::Item>,
        Self::Context: AsyncContext<Self>,
    {
        if ctx.state() == ActorState::Stopped {
            error!("Context::add_stream called for stopped actor.");
            SpawnHandle::default()
        } else {
            ctx.spawn(ActorStream::new(stream))
        }
    }
}

pin_project! {
    pub(crate) struct ActorStream<S> {
        #[pin]
        stream: S,
        started: bool,
    }
}

impl<S> ActorStream<S> {
    pub fn new(fut: S) -> Self {
        Self {
            stream: fut,
            started: false,
        }
    }
}

impl<A, S> ActorFuture<A> for ActorStream<S>
where
    S: Stream,
    A: Actor + StreamHandler<S::Item>,
    A::Context: AsyncContext<A>,
{
    type Output = ();

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        let mut this = self.project();

        if !*this.started {
            *this.started = true;
            <A as StreamHandler<S::Item>>::started(act, ctx);
        }

        let mut polled = 0;

        while let Some(msg) = ready!(this.stream.as_mut().poll_next(task)) {
            A::handle(act, msg, ctx);

            polled += 1;

            if ctx.waiting() {
                return Poll::Pending;
            } else if polled == 16 {
                // Yield after 16 consecutive polls on this stream and self wake up.
                // This is to prevent starvation of other actor futures when this stream yield
                // too many item in short period of time.
                task.waker().wake_by_ref();
                return Poll::Pending;
            }
        }

        A::finished(act, ctx);
        Poll::Ready(())
    }
}
