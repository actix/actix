#![allow(dead_code)]

use std;
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;

use futures::{Async, AsyncSink, Future, Poll, Sink};
use tokio_core::reactor::Handle;

use ctx::CtxMessage;

bitflags!(
    /// State Bitflags
    struct State: u16 {
        /// Service is started
        const STARTED = 0b00000001;
    }
);

pub enum CtxSinkResult<T: SinkContextAware> {
    Sent,
    Err(<<T as SinkContextAware>::Message as CtxMessage>::Error),
    SinkErr(<T::Message as CtxMessage>::Error)
}


pub trait SinkContextAware: Sized + 'static {

    type Context;
    type Message: CtxMessage;
    type Result: CtxMessage;

    /// Create new context for `Context` and stream `S` and run
    fn run<S>(self, ctx: Self::Context, sink: S, handle: &Handle)
        where S: Sink<SinkItem=<<Self as SinkContextAware>::Message as CtxMessage>::Item,
                      SinkError=<<Self as SinkContextAware>::Message as CtxMessage>::Error> + 'static,
    {
        CtxSinkService {
            flags: State::empty(),
            ctx: Rc::new(RefCell::new(ctx)),
            exec: self,
            handle: handle.clone(),
            sink: Box::new(sink),
            sink_items: VecDeque::new(),
            sink_flushed: true,
        }.run()
    }

    /// Method is called when service get polled first time.
    fn start(&mut self, _ctx: &mut Self::Context, _srv: &mut CtxSinkService<Self>) {}

    /// Method is called when wrapped stream completes.
    fn finished(&mut self, ctx: &mut Self::Context, srv: &mut CtxSinkService<Self>)
                -> Result<Async<<<Self as SinkContextAware>::Result as CtxMessage>::Item>,
                          <<Self as SinkContextAware>::Result as CtxMessage>::Error>;

    fn call(&mut self,
            ctx: &mut Self::Context,
            srv: &mut CtxSinkService<Self>, CtxSinkResult<Self>) ->
        Result<Async<<<Self as SinkContextAware>::Result as CtxMessage>::Item>,
               <Self::Result as CtxMessage>::Error>;
}

pub struct CtxSinkService<T> where T: SinkContextAware
{
    flags: State,
    ctx: Rc<RefCell<T::Context>>,
    handle: Handle,
    exec: T,
    sink: Box<Sink<SinkItem=<T::Message as CtxMessage>::Item,
                   SinkError=<T::Message as CtxMessage>::Error>>,
    sink_items: VecDeque<<T::Message as CtxMessage>::Item>,
    sink_flushed: bool,
}

impl<T> CtxSinkService<T> where T: SinkContextAware,
{
    pub fn run(self) where Self: 'static, T: 'static
    {
        let handle: &Handle = unsafe{std::mem::transmute(&self.handle)};
        handle.spawn(self.map(|_| ()).map_err(|_| ()))
    }

    pub fn send(&mut self, item: <T::Message as CtxMessage>::Item)
                -> Result<(), <T::Message as CtxMessage>::Item>
    {
        if self.sink_items.is_empty() {
            self.sink_items.push_back(item);
            Ok(())
        } else {
            Err(item)
        }
    }

    pub fn send_buffered(&mut self, item: <T::Message as CtxMessage>::Item) {
        self.sink_items.push_back(item);
    }
}


impl<T> Future for CtxSinkService<T>
    where T: SinkContextAware
{
    type Item = <<T as SinkContextAware>::Result as CtxMessage>::Item;
    type Error = <<T as SinkContextAware>::Result as CtxMessage>::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let srv: &mut CtxSinkService<T> = unsafe {
            std::mem::transmute(self as &mut CtxSinkService<T>)
        };
        let ctx = &mut *self.ctx.borrow_mut();

        if !self.flags.contains(STARTED) {
            self.flags |= STARTED;
            SinkContextAware::start(&mut self.exec, ctx, srv);
        }

        loop {
            let mut not_ready = true;

            // send sink items
            loop {
                if let Some(item) = self.sink_items.pop_front() {
                    match self.sink.start_send(item) {
                        Ok(AsyncSink::NotReady(item)) => {
                            self.sink_items.push_front(item);
                        }
                        Ok(AsyncSink::Ready) => {
                            self.sink_flushed = false;
                            continue
                        }
                        Err(err) => {
                            match SinkContextAware::call(
                                &mut self.exec, ctx, srv, CtxSinkResult::SinkErr(err))
                            {
                                Ok(Async::NotReady) => (),
                                val => return val,
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
                        match SinkContextAware::call(
                            &mut self.exec, ctx, srv, CtxSinkResult::Sent)
                        {
                            Ok(Async::NotReady) => (),
                            val => return val,
                        }
                    }
                    Ok(Async::NotReady) => (),
                    Err(err) => {
                        match SinkContextAware::call(
                            &mut self.exec, ctx, srv, CtxSinkResult::SinkErr(err))
                        {
                            Ok(Async::NotReady) => (),
                            val => return val,
                        }
                    }
                };
            }

            // are we done
            if not_ready {
                return Ok(Async::NotReady)
            }
        }
    }
}
