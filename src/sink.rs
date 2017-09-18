#![allow(dead_code)]

use std;
use std::collections::VecDeque;

use futures::{Async, AsyncSink, Poll, Sink};
use ctx::{CtxContext, CtxService, Message};

pub enum CtxSinkResult<T: SinkContext> {
    Sent,
    SinkErr(<T::SinkMessage as Message>::Error)
}

pub trait SinkContext: Sized {

    type Context: CtxContext;
    type SinkMessage: Message;

    /// process sink response
    fn call(&mut self,
            _st: &mut <Self::Context as CtxContext>::State,
            _ctx: &mut Self::Context,
            _srv: &mut CtxSinkService<Self>,
            _result: CtxSinkResult<Self>)
            -> Poll<<<Self::Context as CtxContext>::Result as Message>::Item,
                    <<Self::Context as CtxContext>::Result as Message>::Error>
    {
        Ok(Async::NotReady)
    }
}

pub struct CtxSink<T> where T: SinkContext {
    srv: *mut CtxSinkService<T>
}

impl<T> CtxSink<T> where T: SinkContext {
    pub(crate) fn new(srv: *mut CtxSinkService<T>) -> CtxSink<T> {
        CtxSink{srv: srv as *mut _}
    }

    pub fn send(&mut self, item: <T::SinkMessage as Message>::Item)
                -> Result<(), <T::SinkMessage as Message>::Item>
    {
        unsafe {
            (&mut *self.srv).send(item)
        }
    }

    pub fn send_buffered(&mut self, item: <T::SinkMessage as Message>::Item)
    {
        unsafe {
            (&mut *self.srv).send_buffered(item)
        }
    }
}

pub struct CtxSinkService<T> where T: SinkContext
{
    ctx: T,
    sink: Box<Sink<SinkItem=<T::SinkMessage as Message>::Item,
                   SinkError=<T::SinkMessage as Message>::Error>>,
    sink_items: VecDeque<<T::SinkMessage as Message>::Item>,
    sink_flushed: bool,
}

impl<T> CtxSinkService<T> where T: SinkContext
{
    pub(crate) fn new<S>(ctx: T, sink: S) -> CtxSinkService<T>
        where S: Sink<SinkItem=<T::SinkMessage as Message>::Item,
                      SinkError=<T::SinkMessage as Message>::Error> + 'static
    {
        CtxSinkService {
            ctx: ctx,
            sink: Box::new(sink),
            sink_items: VecDeque::new(),
            sink_flushed: true,
        }
    }

    pub fn send(&mut self, item: <T::SinkMessage as Message>::Item)
                -> Result<(), <T::SinkMessage as Message>::Item>
    {
        if self.sink_items.is_empty() {
            self.sink_items.push_back(item);
            Ok(())
        } else {
            Err(item)
        }
    }

    pub fn send_buffered(&mut self, item: <T::SinkMessage as Message>::Item) {
        self.sink_items.push_back(item);
    }
}

pub(crate) trait CtxSinkContextService {

    type Context: CtxContext;

    fn poll(&mut self,
            st: &mut <Self::Context as CtxContext>::State,
            ctx: &mut Self::Context,
            srv: &mut CtxService<Self::Context>)
            -> Poll<<<Self::Context as CtxContext>::Result as Message>::Item,
                    <<Self::Context  as CtxContext>::Result as Message>::Error>;
}


impl<T> CtxSinkContextService for CtxSinkService<T> where T: SinkContext {

    type Context = T::Context;

    fn poll(&mut self,
            st: &mut <Self::Context as CtxContext>::State,
            ctx: &mut Self::Context,
            _srv: &mut CtxService<Self::Context>)
            -> Poll<<<Self::Context as CtxContext>::Result as Message>::Item,
                    <<Self::Context as CtxContext>::Result as Message>::Error>
    {
        let srv: &mut CtxSinkService<T> = unsafe {
            std::mem::transmute(self as &mut CtxSinkService<T>)
        };

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
                            match SinkContext::call(
                                &mut self.ctx, st, ctx, srv, CtxSinkResult::SinkErr(err))
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
                        match SinkContext::call(
                            &mut self.ctx, st, ctx, srv, CtxSinkResult::Sent)
                        {
                            Ok(Async::NotReady) => (),
                            val => return val,
                        }
                    }
                    Ok(Async::NotReady) => (),
                    Err(err) => {
                        match SinkContext::call(
                            &mut self.ctx, st, ctx, srv, CtxSinkResult::SinkErr(err))
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
