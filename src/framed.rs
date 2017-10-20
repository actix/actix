#![allow(dead_code)]

use std;
use std::collections::VecDeque;

use futures::{Async, AsyncSink, Future, Poll, Sink, Stream};
use futures::sync::oneshot::Sender as SyncSender;
use tokio_core::reactor::Handle;
use tokio_io::AsyncRead;
use tokio_io::codec::{Framed, Encoder, Decoder};

use fut::ActorFuture;

use actor::{Actor, Supervised,
            Handler, ResponseType, StreamHandler, SpawnHandle,
            FramedActor, ActorState, ActorContext, AsyncContext};
use address::{Subscriber};
use context::{ActorAddressCell, ActorItemsCell, ActorWaitCell, AsyncContextApi};
use envelope::{Envelope, ToEnvelope, RemoteEnvelope};
use message::Response;


/// Actor execution context for
/// [Framed](https://docs.rs/tokio-io/0.1.3/tokio_io/codec/struct.Framed.html) object
pub struct FramedContext<A>
    where A: FramedActor + Actor<Context=FramedContext<A>>,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    act: A,
    state: ActorState,
    address: ActorAddressCell<A>,
    framed: Option<ActorFramedCell<A>>,
    wait: ActorWaitCell<A>,
    items: ActorItemsCell<A>,
}

type ToEnvelopeSender<A, M> = SyncSender<Result<<A as ResponseType<M>>::Item,
                                                <A as ResponseType<M>>::Error>>;

impl<A, M> ToEnvelope<A, FramedContext<A>, M> for A
    where M: Send + 'static,
          A: FramedActor + Actor<Context=FramedContext<A>> + Handler<M>,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
          <A as ResponseType<M>>::Item: Send, <A as ResponseType<M>>::Item: Send
{
    fn pack(msg: M, tx: Option<ToEnvelopeSender<A, M>>) -> Envelope<A>
    {
        Envelope::new(RemoteEnvelope::new(msg, tx))
    }
}

impl<A> ActorContext<A> for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    /// Stop actor execution
    ///
    /// This method closes actor address and framed object.
    fn stop(&mut self) {
        self.close();
        self.address.close();
        if self.state == ActorState::Running {
            self.state = ActorState::Stopping;
        }
    }

    /// Terminate actor execution
    fn terminate(&mut self) {
        self.close();
        self.address.close();
        self.items.close();
        self.state = ActorState::Stopped;
    }

    /// Actor execution state
    fn state(&self) -> ActorState {
        self.state
    }
}

impl<A> AsyncContext<A> for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.items.spawn(fut)
    }

    fn wait<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.wait.add(fut)
    }

    fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.items.cancel_future(handle)
    }
}

impl<A> AsyncContextApi<A> for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    fn address_cell(&mut self) -> &mut ActorAddressCell<A> {
        &mut self.address
    }
}

impl<A> FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    /// Send item to sink. If sink is closed item returned as an error.
    pub fn send(&mut self, msg: <<A as FramedActor>::Codec as Encoder>::Item)
                -> Result<(), <<A as FramedActor>::Codec as Encoder>::Item>
    {
        if let Some(ref mut framed) = self.framed {
            framed.send(msg);
            Ok(())
        } else {
            Err(msg)
        }
    }

    /// Gracefully close Framed object. FramedContext
    /// will try to send all buffered items and then close.
    /// FramedContext::stop() could be used to force stop sending process.
    pub fn close(&mut self) {
        if let Some(ref mut framed) = self.framed.take() {
            framed.close();
        }
    }
}

impl<A> FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    #[doc(hidden)]
    pub fn subscriber<M: 'static>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>
    {
        Box::new(self.address.unsync_address())
    }

    #[doc(hidden)]
    pub fn sync_subscriber<M: 'static + Send>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              <A as ResponseType<M>>::Item: Send,
              <A as ResponseType<M>>::Error: Send,
    {
        Box::new(self.address.sync_address())
    }
}

impl<A> FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    pub(crate) fn new(act: A, io: <A as FramedActor>::Io,
                      codec: <A as FramedActor>::Codec) -> FramedContext<A>
    {
        FramedContext {
            act: act,
            state: ActorState::Started,
            address: ActorAddressCell::default(),
            framed: Some(ActorFramedCell::new(io.framed(codec))),
            wait: ActorWaitCell::default(),
            items: ActorItemsCell::default(),
        }
    }

    pub(crate) fn run(self, handle: &Handle) {
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
    }

    pub(crate) fn restarting(&mut self) where A: Supervised {
        let ctx: &mut FramedContext<A> = unsafe {
            std::mem::transmute(self as &mut FramedContext<A>)
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
impl<A> Future for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut FramedContext<A> = unsafe {
            std::mem::transmute(self as &mut FramedContext<A>)
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

        // check wait futures
        if let Ok(Async::NotReady) = self.wait.poll(&mut self.act, ctx) {
            return Ok(Async::NotReady)
        }

        let mut prep_stop = false;
        loop {
            let mut not_ready = true;

            // messages
            if let Ok(Async::Ready(_)) = self.address.poll(&mut self.act, ctx) {
                not_ready = false
            }

            // framed
            let closed = if let Some(ref mut framed) = self.framed {
                match framed.poll(&mut self.act, ctx) {
                    Ok(Async::Ready(_)) | Err(_) => {
                        not_ready = false;
                        true
                    },
                    _ => false
                }
            } else { false };
            if closed {
                self.framed.take();
            }

            // check secondary streams
            self.items.poll(&mut self.act, ctx);

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
                        if self.framed.is_some() ||
                            self.address.connected() ||
                            !self.items.is_empty()
                        {
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
                    if !self.framed.is_some() && !self.address.connected() &&
                        self.items.is_empty()
                    {
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

impl<A> std::fmt::Debug for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f,
               "FramedContext({:?}: actor:{:?}) {{ state: {:?}, connected: {}, items: {} }}",
               self as *const _,
               &self.act as *const _,
               self.state, "-", self.items.is_empty())
    }
}

/// Framed object wrapper
pub(crate)
struct ActorFramedCell<A>
    where A: Actor + FramedActor,
          A::Context: AsyncContext<A>,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    started: bool,
    closing: bool,
    response: Option<Response<A, <<A as FramedActor>::Codec as Decoder>::Item>>,
    framed: Framed<<A as FramedActor>::Io, <A as FramedActor>::Codec>,
    stream_closed: bool,
    sink_closed: bool,
    sink_flushed: bool,
    sink_items: VecDeque<<<A as FramedActor>::Codec as Encoder>::Item>,
}

impl<A> ActorFramedCell<A>
    where A: Actor + FramedActor,
          A::Context: AsyncContext<A>,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    pub fn new(framed: Framed<<A as FramedActor>::Io, <A as FramedActor>::Codec>)
               -> ActorFramedCell<A>
    {
        ActorFramedCell {
            started: false,
            closing: false,
            response: None,
            framed: framed,
            stream_closed: false,
            sink_closed: false,
            sink_flushed: true,
            sink_items: VecDeque::new(),
        }
    }

    pub fn alive(&self) -> bool {
        self.sink_closed || self.stream_closed
    }

    pub fn close(&mut self) {
        self.closing = true;
    }

    pub fn send(&mut self, msg: <<A as FramedActor>::Codec as Encoder>::Item) {
        self.sink_items.push_back(msg);
    }
}

impl<A> ActorFuture for ActorFramedCell<A>
    where A: Actor + FramedActor,
          A::Context: AsyncContext<A>,
          A: StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                           <<A as FramedActor>::Codec as Decoder>::Error>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error>
    {
        if !self.started {
            self.started = true;
            <A as StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                                <<A as FramedActor>::Codec as Decoder>::Error>>
                ::started(act, ctx);
        }

        loop {
            let mut not_ready = true;

            if let Some(mut fut) = self.response.take() {
                match fut.poll(act, ctx) {
                    Ok(Async::NotReady) => {
                        self.response = Some(fut);
                    },
                    Ok(Async::Ready(_)) => (),
                    Err(_) => {
                        self.stream_closed = true;
                    }
                }
            }

            // framed stream
            if !self.closing && !self.stream_closed && self.response.is_none() {
                match self.framed.poll() {
                    Ok(Async::Ready(Some(msg))) => {
                        let fut =
                            <Self::Actor as
                             Handler<<<A as FramedActor>::Codec as Decoder>::Item,
                                     <<A as FramedActor>::Codec as Decoder>::Error>>
                            ::handle(act, msg, ctx);
                        self.response = Some(fut);
                        not_ready = false;
                    }
                    Ok(Async::Ready(None)) => {
                        <A as StreamHandler<<<A as FramedActor>::Codec as Decoder>::Item,
                                            <<A as FramedActor>::Codec as Decoder>::Error>>
                            ::finished(act, ctx);
                        self.sink_closed = true;
                        self.stream_closed = true;
                    }
                    Ok(Async::NotReady) => (),
                    Err(err) => {
                        self.sink_closed = true;
                        self.stream_closed = true;
                        <Self::Actor as Handler<<<A as FramedActor>::Codec as Decoder>::Item,
                                                <<A as FramedActor>::Codec as Decoder>::Error>>
                            ::error(act, err, ctx);
                    }
                }
            }

            if !self.sink_closed {
                // send sink items
                loop {
                    if let Some(msg) = self.sink_items.pop_front() {
                        match self.framed.start_send(msg) {
                            Ok(AsyncSink::NotReady(msg)) => {
                                self.sink_items.push_front(msg);
                            }
                            Ok(AsyncSink::Ready) => {
                                self.sink_flushed = false;
                                continue
                            }
                            Err(err) => {
                                self.sink_closed = true;
                                self.sink_flushed = true;
                                <A as FramedActor>::error(act, err, ctx);
                                break
                            }
                        }
                    }
                    break
                }

                // flush sink
                if !self.sink_flushed {
                    match self.framed.poll_complete() {
                        Ok(Async::Ready(_)) => {
                            not_ready = false;
                            self.sink_flushed = true;
                        }
                        Ok(Async::NotReady) => (),
                        Err(err) => {
                            self.sink_closed = true;
                            self.sink_flushed = true;
                            <A as FramedActor>::error(act, err, ctx);
                        }
                    }
                }

                if self.closing && self.sink_flushed && self.sink_items.is_empty() {
                    self.sink_closed = true;
                }
            }

            // are we done
            if !not_ready {
                continue
            }

            if self.stream_closed && self.sink_closed {
                return Ok(Async::Ready(()))
            } else {
                return Ok(Async::NotReady)
            }
        }
    }
}
