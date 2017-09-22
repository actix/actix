#![allow(dead_code)]

use std;
use std::collections::VecDeque;
use futures::{self, Async, AsyncSink, Poll};

use service::{Service, Item};

/// Sink operation result
pub enum SinkResult<T: SinkService> {
    Sent,
    SinkErr(<T::SinkMessage as Item>::Error)
}

/// Service tied to Sink
pub trait SinkService: Sized {

    type Service: Service;
    type SinkMessage: Item;

    /// process sink result
    fn call(&mut self,
            _srv: &mut Self::Service,
            _ctx: &mut SinkContext<Self>,
            _result: SinkResult<Self>)
            -> Poll<<<Self::Service as Service>::Result as Item>::Item,
                    <<Self::Service as Service>::Result as Item>::Error>
    {
        Ok(Async::NotReady)
    }
}

pub struct Sink<T> where T: SinkService {
    srv: *mut SinkContext<T>
}

impl<T> Sink<T> where T: SinkService {
    pub(crate) fn new(srv: *mut SinkContext<T>) -> Sink<T> {
        Sink{srv: srv as *mut _}
    }

    pub fn send(&mut self, item: <T::SinkMessage as Item>::Item)
                -> Result<(), <T::SinkMessage as Item>::Item>
    {
        unsafe {
            (&mut *self.srv).send(item)
        }
    }

    pub fn send_buffered(&mut self, item: <T::SinkMessage as Item>::Item)
    {
        unsafe {
            (&mut *self.srv).send_buffered(item)
        }
    }
}

pub struct SinkContext<T> where T: SinkService
{
    srv: T,
    sink: Box<futures::Sink<SinkItem=<T::SinkMessage as Item>::Item,
                            SinkError=<T::SinkMessage as Item>::Error>>,
    sink_items: VecDeque<<T::SinkMessage as Item>::Item>,
    sink_flushed: bool,
}

/// SinkService execution context object
impl<T> SinkContext<T> where T: SinkService
{
    pub(crate) fn new<S>(srv: T, sink: S) -> SinkContext<T>
        where S: futures::Sink<SinkItem=<T::SinkMessage as Item>::Item,
                               SinkError=<T::SinkMessage as Item>::Error> + 'static
    {
        SinkContext {
            srv: srv,
            sink: Box::new(sink),
            sink_items: VecDeque::new(),
            sink_flushed: true,
        }
    }

    pub fn send(&mut self, item: <T::SinkMessage as Item>::Item)
                -> Result<(), <T::SinkMessage as Item>::Item>
    {
        if self.sink_items.is_empty() {
            self.sink_items.push_back(item);
            Ok(())
        } else {
            Err(item)
        }
    }

    pub fn send_buffered(&mut self, item: <T::SinkMessage as Item>::Item) {
        self.sink_items.push_back(item);
    }
}

pub(crate) trait SinkContextService {

    type Service: Service;

    fn poll(&mut self,
            srv: &mut Self::Service,
            ctx: &mut <<Self as SinkContextService>::Service as Service>::Context)
            -> Poll<<<Self::Service as Service>::Result as Item>::Item,
                    <<Self::Service as Service>::Result as Item>::Error>;
}


impl<T> SinkContextService for SinkContext<T> where T: SinkService {

    type Service = T::Service;

    fn poll(&mut self,
            srv: &mut Self::Service,
            _ctx: &mut <<Self as SinkContextService>::Service as Service>::Context)
            -> Poll<<<Self::Service as Service>::Result as Item>::Item,
                    <<Self::Service as Service>::Result as Item>::Error>
    {
        let ctx: &mut SinkContext<T> = unsafe {
            std::mem::transmute(self as &mut SinkContext<T>)
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
                        Err(err) => match SinkService::call(
                            &mut self.srv, srv, ctx, SinkResult::SinkErr(err))
                        {
                            Ok(Async::NotReady) => (),
                            val => return val,
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
                        match SinkService::call(&mut self.srv, srv, ctx, SinkResult::Sent)
                        {
                            Ok(Async::NotReady) => (),
                            val => return val,
                        }
                    }
                    Ok(Async::NotReady) => (),
                    Err(err) => match SinkService::call(
                        &mut self.srv, srv, ctx, SinkResult::SinkErr(err))
                    {
                        Ok(Async::NotReady) => (),
                        val => return val,
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
