use std::collections::VecDeque;
use futures::{self, Async, AsyncSink};
use futures::unsync::oneshot::{channel, Sender};

use actor::Actor;
use address::{Subscriber, AsyncSubscriber};
use context::Context;
use message::CallResult;


pub struct Sink<I, E> {
    srv: *mut SinkContext<I, E>
}

impl<I, E> Sink<I, E> {
    pub(crate) fn new(srv: *mut SinkContext<I, E>) -> Sink<I, E> {
        Sink{srv: srv as *mut _}
    }
}

impl<I: 'static, E> Subscriber<I> for Sink<I, E> {

    fn send(&self, msg: I) {
        unsafe {(&mut *self.srv).send(msg)}
    }

    fn unbuffered_send(&self, msg: I) -> Result<(), I> {
        unsafe {(&mut *self.srv).unbuffered_send(msg)}
    }
}

impl<I, E> AsyncSubscriber<I> for Sink<I, E> {

    type Future = CallResult<(), E>;

    fn call(&self, msg: I) -> Self::Future {
        unsafe {(&mut *self.srv).call(msg)}
    }

    fn unbuffered_call(&self, msg: I) -> Result<Self::Future, I> {
        unsafe {(&mut *self.srv).unbuffered_call(msg)}
    }
}

enum IoItem<I, E> {
    Message(I),
    Call((I, Sender<Result<(), E>>)),
}

pub(crate) struct SinkContext<I, E>
{
    sink: Box<futures::Sink<SinkItem=I, SinkError=E>>,
    sink_items: VecDeque<IoItem<I, E>>,
    sink_flushed: bool,
}

/// Sink execution context
impl<I, E> SinkContext<I, E>
{
    pub(crate) fn new<S>(sink: S) -> SinkContext<I, E>
        where S: futures::Sink<SinkItem=I, SinkError=E> + 'static
    {
        SinkContext {
            sink: Box::new(sink),
            sink_items: VecDeque::new(),
            sink_flushed: true,
        }
    }

    pub fn call(&mut self, msg: I) -> CallResult<(), E> {
        let (tx, rx) = channel();
        self.sink_items.push_back(IoItem::Call((msg, tx)));

        CallResult::new(rx)
    }

    pub fn unbuffered_call(&mut self, msg: I) -> Result<CallResult<(), E>, I> {
        if self.sink_items.is_empty() {
            let (tx, rx) = channel();
            self.sink_items.push_back(IoItem::Call((msg, tx)));

            Ok(CallResult::new(rx))
        } else {
            Err(msg)
        }
    }

    pub fn send(&mut self, msg: I) {
        self.sink_items.push_back(IoItem::Message(msg));
    }

    pub fn unbuffered_send(&mut self, msg: I) -> Result<(), I> {
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

impl<A, I, E> SinkContextService<A> for SinkContext<I, E>
    where A: Actor
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
                        },
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
