use futures::{Async, Poll, Stream};
use log::error;
use std::marker::PhantomData;

use crate::actor::{
    Actor, ActorContext, ActorState, AsyncContext, Running, SpawnHandle,
};
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
pub trait StreamHandler<I, E>
where
    Self: Actor,
{
    /// Method is called for every message received by this Actor
    fn handle(&mut self, item: I, ctx: &mut Self::Context);

    /// Method is called when stream get polled first time.
    fn started(&mut self, ctx: &mut Self::Context) {}

    /// Method is called when stream emits error.
    ///
    /// If this method returns `ErrorAction::Continue` stream processing
    /// continues otherwise stream processing stops. Default method
    /// implementation returns `ErrorAction::Stop`
    fn error(&mut self, err: E, ctx: &mut Self::Context) -> Running {
        Running::Stop
    }

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
    /// # use std::io;
    /// use actix::prelude::*;
    /// use futures::stream::once;
    ///
    /// #[derive(Message)]
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl StreamHandler<Ping, io::Error> for MyActor {
    ///
    ///     fn handle(&mut self, item: Ping, ctx: &mut Context<MyActor>) {
    ///         println!("PING");
    /// #       System::current().stop()
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
    ///        Self::add_stream(once::<Ping, io::Error>(Ok(Ping)), ctx);
    ///    }
    /// }
    /// # fn main() {
    /// #    let sys = System::new("example");
    /// #    let addr = MyActor.start();
    /// #    sys.run();
    /// # }
    /// ```
    fn add_stream<S>(fut: S, ctx: &mut Self::Context) -> SpawnHandle
    where
        Self::Context: AsyncContext<Self>,
        S: Stream<Item = I, Error = E> + 'static,
        I: 'static,
        E: 'static,
    {
        if ctx.state() == ActorState::Stopped {
            error!("Context::add_stream called for stopped actor.");
            SpawnHandle::default()
        } else {
            ctx.spawn(ActorStream::new(fut))
        }
    }
}

pub(crate) struct ActorStream<A, M, E, S> {
    stream: S,
    started: bool,
    act: PhantomData<A>,
    msg: PhantomData<M>,
    error: PhantomData<E>,
}

impl<A, M, E, S> ActorStream<A, M, E, S> {
    pub fn new(fut: S) -> Self {
        Self {
            stream: fut,
            started: false,
            act: PhantomData,
            msg: PhantomData,
            error: PhantomData,
        }
    }
}

impl<A, M, E, S> ActorFuture for ActorStream<A, M, E, S>
where
    S: Stream<Item = M, Error = E>,
    A: Actor + StreamHandler<M, E>,
    A::Context: AsyncContext<A>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(
        &mut self,
        act: &mut A,
        ctx: &mut A::Context,
    ) -> Poll<Self::Item, Self::Error> {
        if !self.started {
            self.started = true;
            <A as StreamHandler<M, E>>::started(act, ctx);
        }

        loop {
            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    A::handle(act, msg, ctx);
                    if ctx.waiting() {
                        return Ok(Async::NotReady);
                    }
                }
                Err(err) => {
                    if A::error(act, err, ctx) == Running::Stop {
                        A::finished(act, ctx);
                        return Ok(Async::Ready(()));
                    }
                }
                Ok(Async::Ready(None)) => {
                    A::finished(act, ctx);
                    return Ok(Async::Ready(()));
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
            }
        }
    }
}
