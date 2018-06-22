use futures::{Async, Poll, Stream};
use std::marker::PhantomData;

use actor::{Actor, ActorContext, ActorState, AsyncContext, SpawnHandle};
use fut::ActorFuture;

/// Stream handler
///
/// This is helper trait that allows to handle `Stream` in
/// a similar way as normal actor messages.
#[allow(unused_variables)]
pub trait StreamHandler2<I, E>
where
    Self: Actor,
{
    /// Method is called for every message received by this Actor
    ///
    /// * Ok(Some(t)) - new element from the stream
    /// * Ok(None) - end of stream
    /// * Err(e) - stream generated the given failure
    fn handle(&mut self, item: Result<Option<I>, E>, ctx: &mut Self::Context);

    /// This method register stream to an actor context and
    /// allows to handle `Stream` in similar way as normal actor messages.
    ///
    /// ```rust
    /// # #[macro_use] extern crate actix;
    /// # extern crate futures;
    /// # use std::io;
    /// use actix::prelude::*;
    /// use futures::stream::once;
    ///
    /// #[derive(Message)]
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl StreamHandler2<Ping, io::Error> for MyActor {
    ///     fn handle(&mut self, msg: io::Result<Option<Ping>>, ctx: &mut Context<MyActor>) {
    ///         match msg {
    ///             Ok(Some(_)) => println!("PING"),
    ///             _ => println!("finished"),
    ///         }
    /// #       System::current().stop();
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    ///
    ///     fn started(&mut self, ctx: &mut Context<Self>) {
    ///         // add stream
    ///         Self::add_stream(once::<Ping, io::Error>(Ok(Ping)), ctx);
    ///     }
    /// }
    /// # fn main() {
    /// #    System::run(|| {
    /// #        let addr = MyActor.start();
    /// #        System::current().stop();
    /// #    });
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
    act: PhantomData<A>,
    msg: PhantomData<M>,
    error: PhantomData<E>,
}

impl<A, M, E, S> ActorStream<A, M, E, S> {
    pub fn new(fut: S) -> Self {
        ActorStream {
            stream: fut,
            act: PhantomData,
            msg: PhantomData,
            error: PhantomData,
        }
    }
}

impl<A, M, E, S> ActorFuture for ActorStream<A, M, E, S>
where
    S: Stream<Item = M, Error = E>,
    A: Actor + StreamHandler2<M, E>,
    A::Context: AsyncContext<A>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(
        &mut self, act: &mut A, ctx: &mut A::Context,
    ) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    A::handle(act, Ok(Some(msg)), ctx);
                    if ctx.state().stopping() {
                        A::handle(act, Ok(None), ctx);
                    }
                    if ctx.waiting() {
                        return Ok(Async::NotReady);
                    }
                }
                Ok(Async::Ready(None)) => {
                    A::handle(act, Ok(None), ctx);
                    return Ok(Async::Ready(()));
                }
                Err(err) => {
                    A::handle(act, Err(err), ctx);
                    if ctx.waiting() {
                        return Ok(Async::NotReady);
                    }
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
            }
        }
    }
}
