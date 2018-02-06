use std::{io, mem};
use std::cell::UnsafeCell;
use std::rc::Rc;
use std::marker::PhantomData;
extern crate bytes;

use self::bytes::BytesMut;
use futures::{Async, Poll};
use tokio_io::AsyncWrite;
use tokio_io::codec::Encoder;

use fut::ActorFuture;
use actor::{Actor, ActorContext, AsyncContext, SpawnHandle};

/// Stream handler
///
/// `StreamHandler` is an extension of a `Handler` with stream specific methods.
#[allow(unused_variables)]
pub trait WriteHandler<E> where Self: Actor, Self::Context: ActorContext
{
    /// Method is called when writer emits error.
    ///
    /// If this method returns `false` writer processing continues otherwise
    /// stream processing stops.
    fn error(&mut self, err: E, ctx: &mut Self::Context) -> bool {
        true
    }

    /// Method is called when writer finishes.
    ///
    /// By default actor's `Context` get stopped.
    fn finished(&mut self, ctx: &mut Self::Context) {
        ctx.stop()
    }
}

bitflags! {
    struct Flags: u8 {
        const CLOSING = 0b0000_0001;
        const CLOSED = 0b0000_0010;
    }
}

const LOW_WATERMARK: usize = 4 * 1024;
const HIGH_WATERMARK: usize = 4 * LOW_WATERMARK;

/// Wrapper for `AsyncWrite` types
pub struct Writer<T: AsyncWrite, E: From<io::Error>> {
    inner: Rc<UnsafeCell<InnerWriter<T, E>>>,
}

struct InnerWriter<T: AsyncWrite, E: From<io::Error>> {
    flags: Flags,
    io: T,
    buffer: BytesMut,
    error: Option<E>,
    low: usize,
    high: usize,
    handle: SpawnHandle,
}

impl<T: AsyncWrite, E: From<io::Error> + 'static> Writer<T, E>
{
    pub fn new<A, C>(io: T, ctx: &mut C) -> Writer<T, E>
        where A: Actor<Context=C> + WriteHandler<E>,
              C: AsyncContext<A>,
              T: 'static
    {
        let inner = Rc::new(UnsafeCell::new(
            InnerWriter {
                flags: Flags::empty(),
                io: io,
                buffer: BytesMut::new(),
                error: None,
                low: LOW_WATERMARK,
                high: HIGH_WATERMARK,
                handle: SpawnHandle::default(),
            }));
        let h = ctx.spawn(WriterFut{inner: Rc::clone(&inner), act: PhantomData});

        let mut writer = Writer{inner: inner};
        writer.as_mut().handle = h;
        writer
    }

    #[inline]
    fn as_ref(&self) -> &InnerWriter<T, E> {
        unsafe{ &*self.inner.get() }
    }

    #[inline]
    fn as_mut(&mut self) -> &mut InnerWriter<T, E> {
        unsafe{ &mut *self.inner.get() }
    }

    /// Gracefully close sink
    ///
    /// Close process is asynchronous.
    pub fn close(&mut self) {
        self.as_mut().flags.insert(Flags::CLOSING);
    }

    /// Check if sink is closed
    pub fn closed(&self) -> bool {
        self.as_ref().flags.contains(Flags::CLOSED)
    }

    /// Set write buffer capacity
    pub fn set_buffer_capacity(&mut self, low: usize, high: usize) {
        self.as_mut().low = low;
        self.as_mut().high = high;
    }

    /// Send item to a sink.
    pub fn write(&mut self, msg: &[u8]) {
        let inner = self.as_mut();
        inner.buffer.extend_from_slice(msg);
    }

    /// `SpawnHandle` for this writer
    pub fn handle(&self) -> SpawnHandle {
        self.as_ref().handle
    }
}


struct WriterFut<T, E, A> where T: AsyncWrite, E: From<io::Error> {
    act: PhantomData<A>,
    inner: Rc<UnsafeCell<InnerWriter<T, E>>>,
}

impl<T: 'static, E: 'static, A> ActorFuture for WriterFut<T, E, A>
    where T: AsyncWrite,
          E: From<io::Error>,
          A: Actor + WriteHandler<E>, A::Context: AsyncContext<A>
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        let inner = unsafe{ &mut *self.inner.get() };
        if let Some(err) = inner.error.take() {
            if act.error(err, ctx) {
                act.finished(ctx);
                return Ok(Async::Ready(()));
            }
        }

        while !inner.buffer.is_empty() {
            match inner.io.write(&inner.buffer) {
                Ok(n) => {
                    if n == 0 && act.error(
                        io::Error::new(io::ErrorKind::WriteZero,
                                       "failed to write frame to transport").into(), ctx) {
                        act.finished(ctx);
                        return Ok(Async::Ready(()))
                    }
                    let _ = inner.buffer.split_to(n);
                },
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if inner.buffer.len() > inner.high {
                        ctx.wait(WriterDrain{inner: Rc::clone(&self.inner), act: PhantomData});
                    }
                    return Ok(Async::NotReady)
                },
                Err(e) => if act.error(e.into(), ctx) {
                    act.finished(ctx);
                    return Ok(Async::Ready(()))
                }
            }
        }

        // Try flushing the underlying IO
        match inner.io.flush() {
            Ok(_) => (),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                return Ok(Async::NotReady)
            },
            Err(e) => if act.error(e.into(), ctx) {
                act.finished(ctx);
                return Ok(Async::Ready(()))
            }
        }

        // close if closing and we dont need to flush any data
        if inner.flags.contains(Flags::CLOSING) {
            inner.flags |= Flags::CLOSED;
            act.finished(ctx);
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }
}

struct WriterDrain<T, E, A> where T: AsyncWrite, E: From<io::Error> {
    act: PhantomData<A>,
    inner: Rc<UnsafeCell<InnerWriter<T, E>>>,
}

impl<T, E, A> ActorFuture for WriterDrain<T, E, A>
    where T: AsyncWrite, E: From<io::Error>, A: Actor, A::Context: AsyncContext<A>
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, _: &mut A, _: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        let inner = unsafe{ &mut *self.inner.get() };
        if inner.error.is_some() {
            return Ok(Async::Ready(()));
        }

        while !inner.buffer.is_empty() {
            match inner.io.write(&inner.buffer) {
                Ok(n) => {
                    if n == 0 {
                        inner.error = Some(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "failed to write frame to transport").into());
                        return Err(())
                    }
                    let _ = inner.buffer.split_to(n);
                },
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return if inner.buffer.len() < inner.low {
                        Ok(Async::Ready(()))
                    } else {
                        Ok(Async::NotReady)
                    };
                },
                Err(e) => {
                    inner.error = Some(e.into());
                    return Err(())
                }
            }
        }
        Ok(Async::Ready(()))
    }
}

/// Wrapper for `AsyncWrite` and `Encoder` types
pub struct FramedWrite<T: AsyncWrite, U: Encoder> {
    enc: U,
    inner: Rc<UnsafeCell<InnerWriter<T, U::Error>>>,
}

impl<T: AsyncWrite, U: Encoder> FramedWrite<T, U>
{
    pub fn new<A, C>(io: T, encoder: U, ctx: &mut C) -> FramedWrite<T, U>
        where A: Actor<Context=C> + WriteHandler<U::Error>,
              C: AsyncContext<A>,
              U::Error: 'static,
              T: 'static
    {
        let inner = Rc::new(UnsafeCell::new(
            InnerWriter {
                flags: Flags::empty(),
                io: io,
                buffer: BytesMut::new(),
                error: None,
                low: LOW_WATERMARK,
                high: HIGH_WATERMARK,
                handle: SpawnHandle::default(),
            }));
        let h = ctx.spawn(WriterFut{inner: Rc::clone(&inner), act: PhantomData});

        let mut writer = FramedWrite{enc: encoder, inner: inner};
        writer.as_mut().handle = h;
        writer
    }

    pub fn from_buffer<A>(io: T, encoder: U, buf: BytesMut, ctx: &mut A::Context)
                          -> FramedWrite<T, U>
        where A: Actor + WriteHandler<U::Error>,
              A::Context: AsyncContext<A>,
              U::Error: 'static,
              T: 'static
    {
        let inner = Rc::new(UnsafeCell::new(
            InnerWriter {
                flags: Flags::empty(),
                io: io,
                buffer: buf,
                error: None,
                low: LOW_WATERMARK,
                high: HIGH_WATERMARK,
                handle: SpawnHandle::default(),
            }));
        let h = ctx.spawn(WriterFut{inner: Rc::clone(&inner), act: PhantomData});

        let mut writer = FramedWrite{enc: encoder, inner: inner};
        writer.as_mut().handle = h;
        writer
    }

    #[inline]
    fn as_ref(&self) -> &InnerWriter<T, U::Error> {
        unsafe{ &*self.inner.get() }
    }

    #[inline]
    fn as_mut(&mut self) -> &mut InnerWriter<T, U::Error> {
        unsafe{ &mut *self.inner.get() }
    }

    /// Gracefully close sink
    ///
    /// Close process is asynchronous.
    pub fn close(&mut self) {
        self.as_mut().flags.insert(Flags::CLOSING);
    }

    /// Check if sink is closed
    pub fn closed(&self) -> bool {
        self.as_ref().flags.contains(Flags::CLOSED)
    }

    /// Set write buffer capacity
    pub fn set_buffer_capacity(&mut self, low: usize, high: usize) {
        self.as_mut().low = low;
        self.as_mut().high = high;
    }

    /// Write item
    pub fn write(&mut self, item: U::Item) {
        let inner: &mut InnerWriter<T, U::Error> = unsafe{ mem::transmute(self.as_mut()) };
        let _ = self.enc.encode(item, &mut inner.buffer).map_err(|e| {
            inner.error = Some(e);
        });
    }

    /// `SpawnHandle` for this writer
    pub fn handle(&self) -> SpawnHandle {
        self.as_ref().handle
    }
}
