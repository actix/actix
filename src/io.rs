use std::cell::RefCell;
use std::io;
use std::marker::PhantomData;
use std::rc::Rc;

use bytes::BytesMut;
use futures::{task, Async, Poll};
use tokio_codec::Encoder;
use tokio_io::AsyncWrite;

use actor::{Actor, ActorContext, AsyncContext, Running, SpawnHandle};
use fut::ActorFuture;

/// Write handler
///
/// `WriteHandler` is a helper for `AsyncWrite` types. Implementation
/// of this trait is required for `Writer` and `FramedWrite` support.
#[allow(unused_variables)]
pub trait WriteHandler<E>
where
    Self: Actor,
    Self::Context: ActorContext,
{
    /// Method is called when writer emits error.
    ///
    /// If this method returns `ErrorAction::Continue` writer processing
    /// continues otherwise stream processing stops.
    fn error(&mut self, err: E, ctx: &mut Self::Context) -> Running {
        Running::Stop
    }

    /// Method is called when writer finishes.
    ///
    /// By default this method stops actor's `Context`.
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
    inner: UnsafeWriter<T, E>,
}

struct UnsafeWriter<T: AsyncWrite, E: From<io::Error>>(
    Rc<RefCell<InnerWriter<E>>>,
    Rc<RefCell<T>>,
);

impl<T: AsyncWrite, E: From<io::Error>> Clone for UnsafeWriter<T, E> {
    fn clone(&self) -> Self {
        UnsafeWriter(self.0.clone(), self.1.clone())
    }
}

struct InnerWriter<E: From<io::Error>> {
    flags: Flags,
    buffer: BytesMut,
    error: Option<E>,
    low: usize,
    high: usize,
    handle: SpawnHandle,
    task: Option<task::Task>,
}

impl<T: AsyncWrite, E: From<io::Error> + 'static> Writer<T, E> {
    pub fn new<A, C>(io: T, ctx: &mut C) -> Writer<T, E>
    where
        A: Actor<Context = C> + WriteHandler<E>,
        C: AsyncContext<A>,
        T: 'static,
    {
        let inner = UnsafeWriter(
            Rc::new(RefCell::new(InnerWriter {
                flags: Flags::empty(),
                buffer: BytesMut::new(),
                error: None,
                low: LOW_WATERMARK,
                high: HIGH_WATERMARK,
                handle: SpawnHandle::default(),
                task: None,
            })),
            Rc::new(RefCell::new(io)),
        );
        let h = ctx.spawn(WriterFut {
            inner: inner.clone(),
            act: PhantomData,
        });

        let writer = Writer { inner };
        writer.inner.0.borrow_mut().handle = h;
        writer
    }

    /// Gracefully close sink
    ///
    /// Close process is asynchronous.
    pub fn close(&mut self) {
        self.inner.0.borrow_mut().flags.insert(Flags::CLOSING);
    }

    /// Check if sink is closed
    pub fn closed(&self) -> bool {
        self.inner.0.borrow().flags.contains(Flags::CLOSED)
    }

    /// Set write buffer capacity
    pub fn set_buffer_capacity(&mut self, low_watermark: usize, high_watermark: usize) {
        let mut inner = self.inner.0.borrow_mut();
        inner.low = low_watermark;
        inner.high = high_watermark;
    }

    /// Send item to a sink.
    pub fn write(&mut self, msg: &[u8]) {
        let mut inner = self.inner.0.borrow_mut();
        inner.buffer.extend_from_slice(msg);
        if let Some(task) = inner.task.take() {
            task.notify();
        }
    }

    /// `SpawnHandle` for this writer
    pub fn handle(&self) -> SpawnHandle {
        self.inner.0.borrow().handle
    }
}

struct WriterFut<T, E, A>
where
    T: AsyncWrite,
    E: From<io::Error>,
{
    act: PhantomData<A>,
    inner: UnsafeWriter<T, E>,
}

impl<T: 'static, E: 'static, A> ActorFuture for WriterFut<T, E, A>
where
    T: AsyncWrite,
    E: From<io::Error>,
    A: Actor + WriteHandler<E>,
    A::Context: AsyncContext<A>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(
        &mut self, act: &mut A, ctx: &mut A::Context,
    ) -> Poll<Self::Item, Self::Error> {
        let mut inner = self.inner.0.borrow_mut();
        if let Some(err) = inner.error.take() {
            if act.error(err, ctx) == Running::Stop {
                act.finished(ctx);
                return Ok(Async::Ready(()));
            }
        }

        let mut io = self.inner.1.borrow_mut();
        inner.task = None;
        while !inner.buffer.is_empty() {
            match io.write(&inner.buffer) {
                Ok(n) => {
                    if n == 0
                        && act.error(
                            io::Error::new(
                                io::ErrorKind::WriteZero,
                                "failed to write frame to transport",
                            ).into(),
                            ctx,
                        ) == Running::Stop
                    {
                        act.finished(ctx);
                        return Ok(Async::Ready(()));
                    }
                    let _ = inner.buffer.split_to(n);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if inner.buffer.len() > inner.high {
                        ctx.wait(WriterDrain {
                            inner: self.inner.clone(),
                            act: PhantomData,
                        });
                    }
                    return Ok(Async::NotReady);
                }
                Err(e) => if act.error(e.into(), ctx) == Running::Stop {
                    act.finished(ctx);
                    return Ok(Async::Ready(()));
                },
            }
        }

        // Try flushing the underlying IO
        match io.flush() {
            Ok(_) => (),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                return Ok(Async::NotReady)
            }
            Err(e) => if act.error(e.into(), ctx) == Running::Stop {
                act.finished(ctx);
                return Ok(Async::Ready(()));
            },
        }

        // close if closing and we don't need to flush any data
        if inner.flags.contains(Flags::CLOSING) {
            inner.flags |= Flags::CLOSED;
            act.finished(ctx);
            Ok(Async::Ready(()))
        } else {
            inner.task = Some(task::current());
            Ok(Async::NotReady)
        }
    }
}

struct WriterDrain<T, E, A>
where
    T: AsyncWrite,
    E: From<io::Error>,
{
    act: PhantomData<A>,
    inner: UnsafeWriter<T, E>,
}

impl<T, E, A> ActorFuture for WriterDrain<T, E, A>
where
    T: AsyncWrite,
    E: From<io::Error>,
    A: Actor,
    A::Context: AsyncContext<A>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, _: &mut A, _: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        let mut inner = self.inner.0.borrow_mut();
        if inner.error.is_some() {
            return Ok(Async::Ready(()));
        }

        let mut io = self.inner.1.borrow_mut();
        while !inner.buffer.is_empty() {
            match io.write(&inner.buffer) {
                Ok(n) => {
                    if n == 0 {
                        inner.error = Some(
                            io::Error::new(
                                io::ErrorKind::WriteZero,
                                "failed to write frame to transport",
                            ).into(),
                        );
                        return Err(());
                    }
                    let _ = inner.buffer.split_to(n);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return if inner.buffer.len() < inner.low {
                        Ok(Async::Ready(()))
                    } else {
                        Ok(Async::NotReady)
                    };
                }
                Err(e) => {
                    inner.error = Some(e.into());
                    return Err(());
                }
            }
        }
        Ok(Async::Ready(()))
    }
}

/// Wrapper for `AsyncWrite` and `Encoder` types
pub struct FramedWrite<T: AsyncWrite, U: Encoder> {
    enc: U,
    inner: UnsafeWriter<T, U::Error>,
}

impl<T: AsyncWrite, U: Encoder> FramedWrite<T, U> {
    pub fn new<A, C>(io: T, enc: U, ctx: &mut C) -> FramedWrite<T, U>
    where
        A: Actor<Context = C> + WriteHandler<U::Error>,
        C: AsyncContext<A>,
        U::Error: 'static,
        T: 'static,
    {
        let inner = UnsafeWriter(
            Rc::new(RefCell::new(InnerWriter {
                flags: Flags::empty(),
                buffer: BytesMut::new(),
                error: None,
                low: LOW_WATERMARK,
                high: HIGH_WATERMARK,
                handle: SpawnHandle::default(),
                task: None,
            })),
            Rc::new(RefCell::new(io)),
        );
        let h = ctx.spawn(WriterFut {
            inner: inner.clone(),
            act: PhantomData,
        });

        let writer = FramedWrite { enc, inner };
        writer.inner.0.borrow_mut().handle = h;
        writer
    }

    pub fn from_buffer<A, C>(
        io: T, enc: U, buffer: BytesMut, ctx: &mut C,
    ) -> FramedWrite<T, U>
    where
        A: Actor<Context = C> + WriteHandler<U::Error>,
        C: AsyncContext<A>,
        U::Error: 'static,
        T: 'static,
    {
        let inner = UnsafeWriter(
            Rc::new(RefCell::new(InnerWriter {
                buffer,
                flags: Flags::empty(),
                error: None,
                low: LOW_WATERMARK,
                high: HIGH_WATERMARK,
                handle: SpawnHandle::default(),
                task: None,
            })),
            Rc::new(RefCell::new(io)),
        );
        let h = ctx.spawn(WriterFut {
            inner: inner.clone(),
            act: PhantomData,
        });

        let writer = FramedWrite { enc, inner };
        writer.inner.0.borrow_mut().handle = h;
        writer
    }

    /// Gracefully close sink
    ///
    /// Close process is asynchronous.
    pub fn close(&mut self) {
        self.inner.0.borrow_mut().flags.insert(Flags::CLOSING);
    }

    /// Check if sink is closed
    pub fn closed(&self) -> bool {
        self.inner.0.borrow().flags.contains(Flags::CLOSED)
    }

    /// Set write buffer capacity
    pub fn set_buffer_capacity(&mut self, low: usize, high: usize) {
        let mut inner = self.inner.0.borrow_mut();
        inner.low = low;
        inner.high = high;
    }

    /// Write item
    pub fn write(&mut self, item: U::Item) {
        let mut inner = self.inner.0.borrow_mut();
        let _ = self.enc.encode(item, &mut inner.buffer).map_err(|e| {
            inner.error = Some(e);
        });
        if let Some(task) = inner.task.take() {
            task.notify();
        }
    }

    /// `SpawnHandle` for this writer
    pub fn handle(&self) -> SpawnHandle {
        self.inner.0.borrow().handle
    }
}
