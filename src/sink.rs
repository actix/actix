use std::collections::VecDeque;
use futures::{self, Async, AsyncSink};
use futures::unsync::oneshot::{channel, Sender};

use context::Context;
use address::Subscriber;
use message::CallResult;
use service::{Message, Service, ServiceResult};


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

    fn call(&self, msg: M) -> CallResult<M> {
        unsafe {(&mut *self.srv).call(msg)}
    }

    fn unbuffered_call(&self, msg: M) -> Result<CallResult<M>, M> {
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

/// SinkService execution context object
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

    pub fn call(&mut self, msg: M) -> CallResult<M> {
        let (tx, rx) = channel();
        self.sink_items.push_back(IoItem::Call((msg, tx)));

        CallResult::new(rx)
    }

    pub fn unbuffered_call(&mut self, msg: M) -> Result<CallResult<M>, M> {
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

pub(crate) trait SinkContextService<S: Service> {

    fn poll(&mut self, srv: &mut S, ctx: &mut Context<S>) -> ServiceResult;

}

impl<M, S> SinkContextService<S> for SinkContext<M>
    where M: Message<Item=()>,
          S: Service
{

    fn poll(&mut self, _srv: &mut S, _: &mut Context<S>) -> ServiceResult
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
                            Err(_) => return ServiceResult::Done,
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
                                return ServiceResult::Done
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
                    Err(_) => (),
                    //match SinkService::call(
                    //    &mut self.srv, srv, ctx, SinkResult::SinkErr(err))
                    //{
                    //    ServiceResult::NotReady => (),
                    //    ServiceResult::Done => return ServiceResult::Done,
                    //}
                };
            }

            // are we done
            if not_ready {
                return ServiceResult::NotReady
            }
        }
    }
}
