use std;

use futures::{self, Async, Future, Poll, Stream};
use futures::unsync::oneshot::Sender;
use tokio_core::reactor::Handle;

use fut::ActorFuture;
use queue::{sync, unsync};

use actor::{Actor, Supervised, Handler, StreamHandler};
use address::{ActorAddress, Address, SyncAddress, Subscriber};
use envelope::Envelope;
use message::Response;
use sink::{Sink, SinkContext, SinkContextService};


/// Actor execution state
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum ActorState {
    /// Actor is started.
    Started,
    /// Actor is running.
    Running,
    /// Actor is stopping.
    Stopping,
    /// Actor is stopped.
    Stopped,
}

/// context protocol
pub(crate) enum ContextProtocol<A: Actor> {
    /// message envelope
    Envelope(Envelope<A>),
    /// Request sync address
    Upgrade(Sender<SyncAddress<A>>),
}

/// Actor execution context
pub struct Context<A> where A: Actor,
{
    act: A,
    state: ActorState,
    items: Vec<IoItem<A>>,
    address: ActorAddressCell<A>,
}

/// io items
enum IoItem<A: Actor> {
    Sink(Box<SinkContextService<A>>),
    SpawnFuture(Box<ActorFuture<Item=(), Error=(), Actor=A>>),
}

impl<A> Context<A> where A: Actor
{
    /// Actor execution state
    pub fn state(&self) -> ActorState {
        self.state
    }

    /// Stop actor execution
    pub fn stop(&mut self) {
        if self.state == ActorState::Running {
            self.address.close();
            self.state = ActorState::Stopping;
        }
    }

    /// Get actor address
    pub fn address<Address>(&mut self) -> Address
        where A: ActorAddress<A, Address>
    {
        <A as ActorAddress<A, Address>>::get(self)
    }

    #[doc(hidden)]
    pub fn subscriber<M: 'static>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>
    {
        Box::new(self.address.unsync_address())
    }

    #[doc(hidden)]
    pub fn sync_subscriber<M: 'static + Send>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              A::Item: Send,
              A::Error: Send,
    {
        self.address::<SyncAddress<_>>().subscriber()
    }

    pub fn spawn<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        if self.state == ActorState::Stopped {
            error!("Context::spawn called for stopped actor.");
        } else {
            self.items.push(IoItem::SpawnFuture(Box::new(fut)))
        }
    }

    /// This method allow to handle Future in similar way as normal actor message.
    ///
    /// ```rust
    /// extern crate actix;
    /// extern crate futures;
    /// extern crate tokio_core;
    ///
    /// use std::time::Duration;
    /// use futures::Future;
    /// use tokio_core::reactor::Timeout;
    /// use actix::prelude::*;
    ///
    /// // Message
    /// struct Ping;
    ///
    /// struct MyActor;
    ///
    /// impl ResponseType<Ping> for MyActor {
    ///     type Item = ();
    ///     type Error = ();
    /// }
    ///
    /// impl Handler<Ping, std::io::Error> for MyActor {
    ///     fn error(&mut self, err: std::io::Error, ctx: &mut Context<MyActor>) {
    ///         println!("Error: {}", err);
    ///     }
    ///     fn handle(&mut self, msg: Ping, ctx: &mut Context<MyActor>) -> Response<Self, Ping> {
    ///         println!("PING");
    ///         Response::Reply(())
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///
    ///    fn started(&mut self, ctx: &mut Context<Self>) {
    ///        ctx.add_future(
    ///            Timeout::new(Duration::new(0, 1000), Arbiter::handle()).unwrap()
    ///                .map(|_| Ping)
    ///        );
    ///    }
    /// }
    /// fn main() {}
    /// ```
    pub fn add_future<F>(&mut self, fut: F)
        where F: Future + 'static,
              A: Handler<F::Item, F::Error>,
    {
        if self.state == ActorState::Stopped {
            error!("Context::add_future called for stopped actor.");
        } else {
            self.spawn(ActorFutureCell::new(fut))
        }
    }

    /// This method is similar to `add_future` but works with streams.
    pub fn add_stream<S>(&mut self, fut: S)
        where S: Stream + 'static,
              A: Handler<S::Item, S::Error> + StreamHandler<S::Item, S::Error>,
    {
        if self.state == ActorState::Stopped {
            error!("Context::add_stream called for stopped actor.");
        } else {
            self.spawn(ActorStreamCell::new(fut))
        }
    }

    pub fn add_sink<S>(&mut self, sink: S) -> Sink<S::SinkItem, S::SinkError>
        where S: futures::Sink + 'static,
    {
        if self.state == ActorState::Stopped {
            error!("Context::add_sink called for stopped actor.");
        }

        let mut srv = Box::new(SinkContext::new(sink));
        let psrv = srv.as_mut() as *mut _;
        self.items.push(IoItem::Sink(srv));

        Sink::new(psrv)
    }
}


impl<A> Context<A> where A: Actor
{
    pub(crate) fn new(act: A) -> Context<A>
    {
        Context {
            act: act,
            state: ActorState::Started,
            items: Vec::new(),
            address: ActorAddressCell::new(),
        }
    }

    pub(crate) fn run(self, handle: &Handle) {
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
    }

    pub(crate) fn alive(&mut self) -> bool {
        if self.state == ActorState::Stopped {
            false
        } else {
            !self.address.connected() && self.items.is_empty()
        }
    }

    pub(crate) fn restarting(&mut self) where A: Supervised {
        let ctx: &mut Context<A> = unsafe {
            std::mem::transmute(self as &mut Context<A>)
        };
        self.act.restarting(ctx);
    }

    pub(crate) fn replace_actor(&mut self, srv: A) -> A {
        std::mem::replace(&mut self.act, srv)
    }

    pub(crate) fn address_cell(&mut self) -> &mut ActorAddressCell<A> {
        &mut self.address
    }

    pub(crate) fn into_inner(self) -> A {
        self.act
    }
}

#[doc(hidden)]
impl<A> Future for Context<A> where A: Actor
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut Context<A> = unsafe {
            std::mem::transmute(self as &mut Context<A>)
        };

        // update state
        match self.state {
            ActorState::Started => {
                Actor::started(&mut self.act, ctx);
                self.state = ActorState::Running;
            },
            ActorState::Stopping => {
                Actor::stopping(&mut self.act, ctx);
            }
            _ => ()
        }

        let mut prep_stop = false;

        loop {
            let mut not_ready = true;

            if let Ok(Async::Ready(_)) = self.address.poll(&mut self.act, ctx) {
                not_ready = false
            }

            // check secondary streams
            let mut idx = 0;
            let mut len = self.items.len();
            loop {
                if idx >= len {
                    break
                }

                let (drop, item) = match self.items[idx] {
                    IoItem::Sink(ref mut sink) => match sink.poll(&mut self.act, ctx) {
                        Async::Ready(_) => (true, None),
                        Async::NotReady => (false, None)
                    },
                    IoItem::SpawnFuture(ref mut fut) => match fut.poll(&mut self.act, ctx) {
                        Ok(val) => match val {
                            Async::Ready(_) => {
                                not_ready = false;
                                (true, None)
                            }
                            Async::NotReady => (false, None),
                        },
                        Err(_) => (true, None)
                    }
                };

                // we have new pollable item
                if let Some(item) = item {
                    self.items.push(item);
                }

                // number of items could be different, context can add more items
                len = self.items.len();

                // item finishes, we need to remove it,
                // replace current item with last item
                if drop {
                    len -= 1;
                    if idx >= len {
                        self.items.pop();
                        break
                    } else {
                        self.items[idx] = self.items.pop().unwrap();
                    }
                } else {
                    idx += 1;
                }
            }

            // are we done
            if !not_ready {
                continue
            }

            // check state
            match self.state {
                ActorState::Stopped => {
                    self.state = ActorState::Stopped;
                    Actor::stopped(&mut self.act, ctx);
                    return Ok(Async::Ready(()))
                },
                ActorState::Stopping => {
                    if prep_stop {
                        if self.address.connected() || !self.items.is_empty() {
                            self.state = ActorState::Running;
                            continue
                        } else {
                            self.state = ActorState::Stopped;
                            Actor::stopped(&mut self.act, ctx);
                            return Ok(Async::Ready(()))
                        }
                    } else {
                        Actor::stopping(&mut self.act, ctx);
                        prep_stop = true;
                        continue
                    }
                },
                ActorState::Running => {
                    if !self.address.connected() && self.items.is_empty() {
                        self.state = ActorState::Stopping;
                        Actor::stopping(&mut self.act, ctx);
                        prep_stop = true;
                        continue
                    }
                },
                _ => (),
            }

            return Ok(Async::NotReady)
        }
    }
}


pub(crate)
struct ActorAddressCell<A> where A: Actor
{
    sync_alive: bool,
    sync_msgs: Option<sync::UnboundedReceiver<Envelope<A>>>,
    unsync_msgs: unsync::UnboundedReceiver<ContextProtocol<A>>,
}

impl<A> ActorAddressCell<A> where A: Actor
{
    fn new() -> ActorAddressCell<A> {
        ActorAddressCell {
            sync_alive: false,
            sync_msgs: None,
            unsync_msgs: unsync::unbounded(),
        }
    }

    pub fn close(&mut self) {
        self.unsync_msgs.close();
        if let Some(ref mut msgs) = self.sync_msgs {
            msgs.close()
        }
    }

    pub fn connected(&mut self) -> bool {
        self.unsync_msgs.connected() || self.sync_alive
    }

    pub fn unsync_sender(&mut self) -> unsync::UnboundedSender<ContextProtocol<A>> {
        self.unsync_msgs.sender()
    }

    pub fn unsync_address(&mut self) -> Address<A> {
        Address::new(self.unsync_msgs.sender())
    }

    pub fn sync_address(&mut self) -> SyncAddress<A> {
        if self.sync_msgs.is_none() {
            let (tx, rx) = sync::unbounded();
            self.sync_msgs = Some(rx);
            self.sync_alive = true;
            SyncAddress::new(tx)
        } else {
            if let Some(ref mut addr) = self.sync_msgs {
                return SyncAddress::new(addr.sender())
            }
            unreachable!();
        }
    }
}

impl<A> ActorFuture for ActorAddressCell<A> where A: Actor
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut Context<A>) -> Poll<Self::Item, Self::Error>
    {
        loop {
            let mut not_ready = true;

            // unsync messages
            match self.unsync_msgs.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    not_ready = false;
                    match msg {
                        ContextProtocol::Envelope(mut env) => {
                            env.handle(act, ctx)
                        }
                        ContextProtocol::Upgrade(tx) => {
                            let _ = tx.send(self.sync_address());
                        }
                    }
                }
                Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => (),
            }

            // sync messages
            if self.sync_alive {
                if let Some(ref mut msgs) = self.sync_msgs {
                    match msgs.poll() {
                        Ok(Async::Ready(Some(mut msg))) => {
                            not_ready = false;
                            msg.handle(act, ctx);
                        }
                        Ok(Async::Ready(None)) | Err(_) => {
                            self.sync_alive = false;
                        },
                        Ok(Async::NotReady) => (),
                    }
                }
            }

            if not_ready {
                return Ok(Async::NotReady)
            }
        }
    }
}


struct ActorFutureCell<A, M, F, E>
    where A: Actor + Handler<M, E>,
          F: Future<Item=M, Error=E>,
{
    act: std::marker::PhantomData<A>,
    fut: F,
    result: Option<Response<A, M>>,
}

impl<A, M, F, E> ActorFutureCell<A, M, F, E>
    where A: Actor + Handler<M, E>,
          F: Future<Item=M, Error=E>,
{
    fn new(fut: F) -> ActorFutureCell<A, M, F, E>
    {
        ActorFutureCell {
            act: std::marker::PhantomData,
            fut: fut,
            result: None }
    }
}

impl<A, M, F, E> ActorFuture for ActorFutureCell<A, M, F, E>
    where A: Actor + Handler<M, E>,
          F: Future<Item=M, Error=E>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut Context<A>) -> Poll<Self::Item, Self::Error>
    {
        loop {
            if let Some(mut fut) = self.result.take() {
                match fut.poll(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.result = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) =>
                        return Ok(Async::Ready(())),
                    Err(_) =>
                        return Err(())
                }
            }

            match self.fut.poll() {
                Ok(Async::Ready(msg)) => {
                    let fut = <Self::Actor as Handler<M, E>>::handle(act, msg, ctx);
                    self.result = Some(fut);
                    continue
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(err) => {
                    <Self::Actor as Handler<M, E>>::error(act, err, ctx);
                    return Err(())
                }
            }
        }
    }
}


struct ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + Handler<M, E> + StreamHandler<M, E>,
{
    act: std::marker::PhantomData<A>,
    started: bool,
    fut: Option<Response<A, M>>,
    stream: S,
}

impl<A, M, E, S> ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E> + 'static,
          A: Actor + Handler<M, E> + StreamHandler<M, E>,
{
    fn new(fut: S) -> ActorStreamCell<A, M, E, S>
    {
        ActorStreamCell {
            act: std::marker::PhantomData,
            started: false,
            fut: None,
            stream: fut }
    }
}

impl<A, M, E, S> ActorFuture for ActorStreamCell<A, M, E, S>
    where S: Stream<Item=M, Error=E>,
          A: Actor + Handler<M, E> + StreamHandler<M, E>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut Context<A>) -> Poll<Self::Item, Self::Error>
    {
        if !self.started {
            self.started = true;
            <A as StreamHandler<M, E>>::started(act, ctx);
        }

        loop {
            if let Some(mut fut) = self.fut.take() {
                match fut.poll(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.fut = Some(fut);
                        return Ok(Async::NotReady)
                    }
                    Ok(Async::Ready(_)) => (),
                    Err(_) => return Err(())
                }
            }

            match self.stream.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    let fut = <Self::Actor as Handler<M, E>>::handle(act, msg, ctx);
                    self.fut = Some(fut);
                    continue
                }
                Ok(Async::Ready(None)) => {
                    <A as StreamHandler<M, E>>::finished(act, ctx);
                    return Ok(Async::Ready(()))
                }
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Err(err) => {
                    <Self::Actor as Handler<M, E>>::error(act, err, ctx);
                    return Err(())
                }
            }
        }
    }
}

/// Helper trait which can spawn future into actor's context
pub trait ContextFutureSpawner<A> where A: Actor {
    /// spawn future into `Context<A>`
    fn spawn(self, fut: &mut Context<A>);
}


impl<A, T> ContextFutureSpawner<A> for T
    where A: Actor,
          T: ActorFuture<Item=(), Error=(), Actor=A> + 'static
{
    fn spawn(self, ctx: &mut Context<A>) {
        ctx.spawn(self)
    }
}
