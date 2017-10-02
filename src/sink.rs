use std::collections::VecDeque;
use futures::{self, Async, AsyncSink, Future, Poll};
use futures::unsync::oneshot::{channel, Canceled, Sender, Receiver};

use actor::{Actor, AsyncContext};
use address::Subscriber;


/// Sink wrapper
pub struct Sink<I, E> {
    // FIXME: Should this be replaced with Rc<RefCell<SinkContext>>
    srv: *mut SinkContext<I, E>
}

impl<I, E> Sink<I, E> {
    pub(crate) fn new(srv: *mut SinkContext<I, E>) -> Sink<I, E> {
        Sink{srv: srv as *mut _}
    }

    pub fn call(&self, msg: I) -> CallResult<(), E> {
        unsafe {(&mut *self.srv).call(msg)}
    }

    pub fn unbuffered_call(&self, msg: I) -> Result<CallResult<(), E>, I> {
        unsafe {(&mut *self.srv).unbuffered_call(msg)}
    }
}

impl<I: 'static, E> Subscriber<I> for Sink<I, E> {

    fn send(&self, msg: I) -> Result<(), I> {
        unsafe {(&mut *self.srv).send(msg)};
        Ok(())
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

    #[allow(dead_code)]
    pub fn unbuffered_send(&mut self, msg: I) -> Result<(), I> {
        if self.sink_items.is_empty() {
            self.sink_items.push_back(IoItem::Message(msg));
            Ok(())
        } else {
            Err(msg)
        }
    }
}

#[must_use = "future do nothing unless polled"]
pub struct CallResult<I, E> {
    rx: Receiver<Result<I, E>>,
}

impl<I, E> CallResult<I, E>
{
    pub(crate) fn new(rx: Receiver<Result<I, E>>) -> CallResult<I, E> {
        CallResult{rx: rx}
    }
}

impl<I, E> Future for CallResult<I, E>
{
    type Item = Result<I, E>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
    {
        self.rx.poll()
    }
}

pub(crate) trait SinkContextService<A: Actor> where A::Context: AsyncContext<A> {

    fn poll(&mut self, srv: &mut A, ctx: &mut A::Context) -> Async<()>;

}

impl<A, I, E> SinkContextService<A> for SinkContext<I, E>
    where A: Actor,
          A::Context: AsyncContext<A>
{
    fn poll(&mut self, _act: &mut A, _: &mut A::Context) -> Async<()>
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
