use std::collections::VecDeque;
use futures::{self, Async, AsyncSink};
use futures::unsync::oneshot::{channel, Sender};

use context::Context;
use actor::{Actor, Message};
use address::{Subscriber, AsyncSubscriber};
use message::CallResult;


pub struct Sink<M> where M: Message {
    srv: *mut SinkContext<M>
}

impl<M> Sink<M> where M: Message {
    pub(crate) fn new(srv: *mut SinkContext<M>) -> Sink<M> {
        Sink{srv: srv as *mut _}
    }
}

impl<M> Subscriber<M> for Sink<M> where M: Message {

    fn send(&self, msg: M) {
        unsafe {(&mut *self.srv).send(msg)}
    }

    fn unbuffered_send(&self, msg: M) -> Result<(), M> {
        unsafe {(&mut *self.srv).unbuffered_send(msg)}
    }
}

impl<M> AsyncSubscriber<M> for Sink<M> where M: Message {

    type Future = CallResult<M::Item, M::Error>;

    fn call(&self, msg: M) -> Self::Future {
        unsafe {(&mut *self.srv).call(msg)}
    }

    fn unbuffered_call(&self, msg: M) -> Result<Self::Future, M> {
        unsafe {(&mut *self.srv).unbuffered_call(msg)}
    }
}

enum IoItem<M> where M: Message {
    Message(M),
    Call((M, Sender<Result<M::Item, M::Error>>)),
}

pub(crate) struct SinkContext<M> where M: Message,
{
    sink: Box<futures::Sink<SinkItem=M, SinkError=M::Error>>,
    sink_items: VecDeque<IoItem<M>>,
    sink_flushed: bool,
}

/// Sink execution context
impl<M> SinkContext<M> where M: Message
{
    pub(crate) fn new<S>(sink: S) -> SinkContext<M>
        where S: futures::Sink<SinkItem=M, SinkError=M::Error> + 'static
    {
        SinkContext {
            sink: Box::new(sink),
            sink_items: VecDeque::new(),
            sink_flushed: true,
        }
    }

    pub fn call(&mut self, msg: M) -> CallResult<M::Item, M::Error> {
        let (tx, rx) = channel();
        self.sink_items.push_back(IoItem::Call((msg, tx)));

        CallResult::new(rx)
    }

    pub fn unbuffered_call(&mut self, msg: M) -> Result<CallResult<M::Item, M::Error>, M> {
        if self.sink_items.is_empty() {
            let (tx, rx) = channel();
            self.sink_items.push_back(IoItem::Call((msg, tx)));

            Ok(CallResult::new(rx))
        } else {
            Err(msg)
        }
    }

    pub fn send(&mut self, msg: M) {
        self.sink_items.push_back(IoItem::Message(msg));
    }

    pub fn unbuffered_send(&mut self, msg: M) -> Result<(), M> {
        if self.sink_items.is_empty() {
            self.sink_items.push_back(IoItem::Message(msg));
            Ok(())
        } else {
            Err(msg)
        }
    }
}

pub(crate) trait SinkContextService<A: Actor> {

    fn poll(&mut self, srv: &mut A, ctx: &mut Context<A>) -> Async<()>;

}

impl<M, A> SinkContextService<A> for SinkContext<M>
    where M: Message<Item=()>,
          A: Actor
{

    fn poll(&mut self, _act: &mut A, _: &mut Context<A>) -> Async<()>
    {
        loop {
            let mut not_ready = true;

            // send sink items
            loop {
                if let Some(item) = self.sink_items.pop_front() {
                    match item {
                        IoItem::Message(msg) => match self.sink.start_send(msg) {
                            Ok(AsyncSink::NotReady(msg)) => {
                                self.sink_items.push_front(IoItem::Message(msg));
                            }
                            Ok(AsyncSink::Ready) => {
                                self.sink_flushed = false;
                                continue
                            }
                            Err(_) => return Async::Ready(()),
                        }
                        IoItem::Call((msg, tx)) => match self.sink.start_send(msg) {
                            Ok(AsyncSink::NotReady(msg)) => {
                                self.sink_items.push_front(IoItem::Call((msg, tx)));
                            }
                            Ok(AsyncSink::Ready) => {
                                let _ = tx.send(Ok(()));
                                self.sink_flushed = false;
                                continue
                            }
                            Err(err) => {
                                let _ = tx.send(Err(err));
                                return Async::Ready(())
                            }
                        }
                    }
                }
                break
            }

            // flush sink
            if !self.sink_flushed {
                match self.sink.poll_complete() {
                    Ok(Async::Ready(_)) => {
                        not_ready = false;
                        self.sink_flushed = true;
                    }
                    Ok(Async::NotReady) => (),
                    Err(_) => return Async::Ready(())
                };
            }

            // are we done
            if not_ready {
                return Async::NotReady
            }
        }
    }
}
