#![allow(dead_code)]

use std;
use std::collections::VecDeque;

use futures::{Async, AsyncSink, Future, Poll, Sink};
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, };
use tokio_io::codec::{Framed, Encoder, Decoder};

use fut::ActorFuture;
// use queue::{sync, unsync};

use actor::{Actor, Supervised,
            Handler, FramedActor, ActorState, AsyncContext, BaseContext};
use address::{Subscriber};
use context::{ActorAddressCell, AsyncContextApi};


/// Actor execution context
pub struct FramedContext<A> where A: FramedActor + Actor<Context=FramedContext<A>>,
{
    act: A,
    state: ActorState,
    address: ActorAddressCell<A>,
    framed: Framed<<A as FramedActor>::Io, <A as FramedActor>::Codec>,
    items: Vec<Box<ActorFuture<Item=(), Error=(), Actor=A>>>,
    sink_items: VecDeque<<<A as FramedActor>::Codec as Encoder>::Item>,
    sink_flushed: bool,
    sink_closed: bool,
}

impl<A> BaseContext<A> for FramedContext<A> where A: Actor<Context=Self> + FramedActor
{
    /// Stop actor execution
    fn stop(&mut self) {
        if self.state == ActorState::Running {
            self.address.close();
            self.state = ActorState::Stopping;
        }
    }

    /// Actor execution state
    fn state(&self) -> ActorState {
        self.state
    }
}

impl<A> AsyncContext<A> for FramedContext<A> where A: Actor<Context=Self> + FramedActor
{
    fn spawn<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        if self.state == ActorState::Stopped {
            error!("Context::spawn called for stopped actor.");
        } else {
            self.items.push(Box::new(fut))
        }
    }
}

impl<A> AsyncContextApi<A> for FramedContext<A> where A: Actor<Context=Self> + FramedActor {
    fn address_cell(&mut self) -> &mut ActorAddressCell<A> {
        &mut self.address
    }
}

impl<A> FramedContext<A> where A: Actor<Context=Self> + FramedActor
{
    #[doc(hidden)]
    pub fn subscriber<M: 'static>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>
    {
        Box::new(self.address.unsync_address())
    }

    /*#[doc(hidden)]
    pub fn sync_subscriber<M: 'static + Send>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              A::Item: Send,
              A::Error: Send,
    {
        self.address::<SyncAddress<_>>().subscriber()
    }*/

    pub fn send(&mut self, msg: <<A as FramedActor>::Codec as Encoder>::Item) {
        self.sink_items.push_back(msg);
    }

    /// Gracefully close sink part of the Framed object. FramedContext
    /// will try to send all buffered items and then close.
    /// FramedContext::stop() could be used to force stop sending process.
    pub fn close(&mut self) {
        // self.sink_items.push_back(msg);
    }
}

impl<A> FramedContext<A> where A: Actor<Context=Self> + FramedActor
{
    pub(crate) fn new(act: A, io: <A as FramedActor>::Io,
                      codec: <A as FramedActor>::Codec) -> FramedContext<A>
        where A: Handler<<<A as FramedActor>::Codec as Decoder>::Item,
                         <<A as FramedActor>::Codec as Decoder>::Error>
    {
        FramedContext {
            act: act,
            state: ActorState::Started,
            address: ActorAddressCell::new(),
            framed: io.framed(codec),
            items: Vec::new(),
            sink_items: VecDeque::new(),
            sink_flushed: true,
            sink_closed: false,
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
impl<A> Future for FramedContext<A> where A: Actor<Context=Self> + FramedActor
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

                let (drop, item) = match self.items[idx].poll(&mut self.act, ctx) {
                    Ok(val) => match val {
                        Async::Ready(_) => {
                            not_ready = false;
                            (true, None)
                        }
                        Async::NotReady => (false, None),
                    },
                    Err(_) => (true, None)
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

            // send sink items
            if !self.sink_closed {
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
                            Err(_) => {
                                self.sink_closed = true;
                                break
                            }
                        }
                    }
                    break
                }
            }

            // flush sink
            if !self.sink_flushed && !self.sink_closed {
                match self.framed.poll_complete() {
                    Ok(Async::Ready(_)) => {
                        not_ready = false;
                        self.sink_flushed = true;
                    }
                    Ok(Async::NotReady) => (),
                    Err(_) => {
                        self.sink_closed = true;
                    }
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
