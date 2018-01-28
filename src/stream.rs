use std::rc::Rc;
use std::cell::Cell;
use std::marker::PhantomData;
use futures::{Async, Poll, Stream};

use fut::ActorFuture;
use actor::{Actor, ActorState, ActorContext, AsyncContext, SpawnHandle};


/// Stream handler
///
/// `StreamHandler` is an extension of a `Handler` with stream specific methods.
#[allow(unused_variables)]
pub trait StreamHandler<I, E> where Self: Actor
{
    /// Method is called when stream get polled first time.
    fn started(&mut self, ctx: &mut Self::Context, handle: SpawnHandle) {}

    /// Method is called for every message received by this Actor
    fn handle(&mut self, item: I, ctx: &mut Self::Context);

    /// Method is called when stream finishes.
    ///
    /// Error indicates if stream finished with error.
    fn finished(&mut self, error: Option<E>, ctx: &mut Self::Context, handle: SpawnHandle) {}

    /// This method is similar to `add_future` but works with streams.
    ///
    /// Information to consider. Actor wont receive next item from a stream
    /// until `Response` future resolves to a result. `Self::reply` resolves immediately.
    ///
    /// This method is similar to `add_stream` but it skips result error.
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
    /// impl StreamHandler<Ping, io::Error> for MyActor {
    ///
    ///     fn handle(&mut self, item: Ping, ctx: &mut Context<MyActor>) {
    ///         println!("PING");
    /// #       Arbiter::system().send(actix::msgs::SystemExit(0));
    ///     }
    ///
    ///     fn finished(&mut self, error: Option<io::Error>,
    ///                 ctx: &mut Self::Context, handle: SpawnHandle) {
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
    /// #    let addr: Address<_> = MyActor.start();
    /// #    sys.run();
    /// # }
    /// ```
    fn add_stream<S>(fut: S, ctx: &mut Self::Context) -> SpawnHandle
        where Self::Context: AsyncContext<Self>,
              S: Stream<Item=I, Error=E> + 'static,
              I: 'static, E: 'static
    {
        if ctx.state() == ActorState::Stopped {
            error!("Context::add_stream called for stopped actor.");
            SpawnHandle::default()
        } else {
            let handle = Rc::new(Cell::new(SpawnHandle::default()));
            let h = ctx.spawn(ActorStream::new(fut, Rc::clone(&handle)));
            handle.as_ref().set(h);
            h
        }
    }
}

pub(crate) struct ActorStream<A, M, E, S> {
    stream: S,
    handle: Rc<Cell<SpawnHandle>>,
    started: bool,
    act: PhantomData<A>,
    msg: PhantomData<M>,
    error: PhantomData<E>,
}

impl<A, M, E, S> ActorStream<A, M, E, S> {
    pub fn new(fut: S, handle: Rc<Cell<SpawnHandle>>) -> Self {
        ActorStream{stream: fut, handle: handle, started: false,
                    act: PhantomData, msg: PhantomData, error: PhantomData}
    }
}

impl<A, M, E, S> ActorFuture for ActorStream<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + StreamHandler<M, E>, A::Context: AsyncContext<A>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        if !self.started {
            self.started = true;
            <A as StreamHandler<M, E>>::started(act, ctx, self.handle.as_ref().get());
        }
        
        loop {
            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    <A as StreamHandler<M, E>>::handle(act, msg, ctx);
                    if ctx.waiting() {
                        return Ok(Async::NotReady)
                    }
                }
                Err(err) => {
                    <A as StreamHandler<M, E>>::finished(
                        act, Some(err), ctx, self.handle.as_ref().get());
                    return Ok(Async::Ready(()))
                },
                Ok(Async::Ready(None)) => {
                    <A as StreamHandler<M, E>>::finished(
                        act, None, ctx, self.handle.as_ref().get());
                    return Ok(Async::Ready(()))
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
            }
        }
    }
}
