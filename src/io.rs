use std::cell::RefCell;
use std::io;
use std::marker::PhantomData;
use std::rc::Rc;

use bitflags::bitflags;
use bytes::BytesMut;
use futures::sink::Sink;
use futures::{task, Async, AsyncSink, Poll, StartSend};
use tokio_codec::Encoder;
use tokio_io::AsyncWrite;

use crate::actor::{Actor, ActorContext, AsyncContext, Running, SpawnHandle};
use crate::fut::ActorFuture;

/// A helper trait for write handling.
///
/// `WriteHandler` is a helper for `AsyncWrite` types. Implementation
/// of this trait is required for `Writer` and `FramedWrite` support.
#[allow(unused_variables)]
pub trait WriteHandler<E>
where
    Self: Actor,
    Self::Context: ActorContext,
{
    /// Called when the writer emits error.
    ///
    /// If this method returns `ErrorAction::Continue` writer processing
    /// continues otherwise stream processing stops.
    fn error(&mut self, err: E, ctx: &mut Self::Context) -> Running {
        Running::Stop
    }

    /// Called when the writer finishes.
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

/// A wrapper for `AsyncWrite` types.
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
    pub fn new<A, C>(io: T, ctx: &mut C) -> Self
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

        let writer = Self { inner };
        writer.inner.0.borrow_mut().handle = h;
        writer
    }

    /// Gracefully closes the sink.
    ///
    /// The closing happens asynchronously.
    pub fn close(&mut self) {
        self.inner.0.borrow_mut().flags.insert(Flags::CLOSING);
    }

    /// Checks if the sink is closed.
    pub fn closed(&self) -> bool {
        self.inner.0.borrow().flags.contains(Flags::CLOSED)
    }

    /// Sets the write buffer capacity.
    pub fn set_buffer_capacity(&mut self, low_watermark: usize, high_watermark: usize) {
        let mut inner = self.inner.0.borrow_mut();
        inner.low = low_watermark;
        inner.high = high_watermark;
    }

    /// Sends an item to the sink.
    pub fn write(&mut self, msg: &[u8]) {
        let mut inner = self.inner.0.borrow_mut();
        inner.buffer.extend_from_slice(msg);
        if let Some(task) = inner.task.take() {
            task.notify();
        }
    }

    /// Returns the `SpawnHandle` for this writer.
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
        &mut self,
        act: &mut A,
        ctx: &mut A::Context,
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
                            )
                            .into(),
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
                Err(e) => {
                    if act.error(e.into(), ctx) == Running::Stop {
                        act.finished(ctx);
                        return Ok(Async::Ready(()));
                    }
                }
            }
        }

        // Try flushing the underlying IO
        match io.flush() {
            Ok(_) => (),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                return Ok(Async::NotReady);
            }
            Err(e) => {
                if act.error(e.into(), ctx) == Running::Stop {
                    act.finished(ctx);
                    return Ok(Async::Ready(()));
                }
            }
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
                            )
                            .into(),
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

/// A wrapper for the `AsyncWrite` and `Encoder` types. The AsyncWrite will be flushed when this
/// struct is dropped.
pub struct FramedWrite<T: AsyncWrite, U: Encoder> {
    enc: U,
    inner: UnsafeWriter<T, U::Error>,
}

impl<T: AsyncWrite, U: Encoder> FramedWrite<T, U> {
    pub fn new<A, C>(io: T, enc: U, ctx: &mut C) -> Self
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

        let writer = Self { enc, inner };
        writer.inner.0.borrow_mut().handle = h;
        writer
    }

    pub fn from_buffer<A, C>(io: T, enc: U, buffer: BytesMut, ctx: &mut C) -> Self
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

        let writer = Self { enc, inner };
        writer.inner.0.borrow_mut().handle = h;
        writer
    }

    /// Gracefully closes the sink.
    ///
    /// The closing happens asynchronously.
    pub fn close(&mut self) {
        self.inner.0.borrow_mut().flags.insert(Flags::CLOSING);
    }

    /// Checks if the sink is closed.
    pub fn closed(&self) -> bool {
        self.inner.0.borrow().flags.contains(Flags::CLOSED)
    }

    /// Sets the write buffer capacity.
    pub fn set_buffer_capacity(&mut self, low: usize, high: usize) {
        let mut inner = self.inner.0.borrow_mut();
        inner.low = low;
        inner.high = high;
    }

    /// Writes an item to the sink.
    pub fn write(&mut self, item: U::Item) {
        let mut inner = self.inner.0.borrow_mut();
        let _ = self.enc.encode(item, &mut inner.buffer).map_err(|e| {
            inner.error = Some(e);
        });
        if let Some(task) = inner.task.take() {
            task.notify();
        }
    }

    /// Returns the `SpawnHandle` for this writer.
    pub fn handle(&self) -> SpawnHandle {
        self.inner.0.borrow().handle
    }
}

impl<T: AsyncWrite, U: Encoder> Drop for FramedWrite<T, U> {
    fn drop(&mut self) {
        // Attempts to write any remaining bytes to the stream and flush it
        let mut async_writer = self.inner.1.borrow_mut();
        let inner = self.inner.0.borrow_mut();
        if !inner.buffer.is_empty() {
            // Results must be ignored during drop, as the errors cannot be handled meaningfully
            let _ = async_writer.write(&inner.buffer);
            let _ = async_writer.flush();
        }
    }
}

/// A wrapper for the `Sink` type.
pub struct SinkWrite<S: Sink> {
    inner: Rc<RefCell<InnerSinkWrite<S>>>,
}

impl<S: Sink + 'static> SinkWrite<S> {
    pub fn new<A, C>(sink: S, ctxt: &mut C) -> Self
    where
        A: Actor<Context = C> + WriteHandler<S::SinkError>,
        C: AsyncContext<A>,
    {
        let inner = Rc::new(RefCell::new(InnerSinkWrite {
            closing_flag: Flags::empty(),
            sink,
            task: None,
            handle: SpawnHandle::default(),
        }));

        let handle = ctxt.spawn(SinkWriteFuture {
            inner: inner.clone(),
            _actor: PhantomData,
        });

        inner.borrow_mut().handle = handle;
        SinkWrite { inner }
    }

    /// Sends an item to the sink.
    pub fn write(&mut self, item: S::SinkItem) -> StartSend<S::SinkItem, S::SinkError> {
        let res = self.inner.borrow_mut().sink.start_send(item);
        match res {
            Err(_) => {} // TODO close or send to inner future ?
            Ok(AsyncSink::Ready) => self.notify_task(),
            Ok(AsyncSink::NotReady(_)) => {}
        }
        res
    }

    /// Gracefully closes the sink.
    ///
    /// The closing happens asynchronously.
    pub fn close(&mut self) {
        self.inner.borrow_mut().closing_flag.insert(Flags::CLOSING);
        self.notify_task();
    }

    /// Checks if the sink is closed.
    pub fn closed(&self) -> bool {
        self.inner.borrow_mut().closing_flag.contains(Flags::CLOSED)
    }

    fn notify_task(&self) {
        if let Some(task) = &self.inner.borrow().task {
            task.notify()
        }
    }

    /// Returns the `SpawnHandle` for this writer.
    pub fn handle(&self) -> SpawnHandle {
        self.inner.borrow().handle
    }
}

struct InnerSinkWrite<S: Sink> {
    closing_flag: Flags,
    sink: S,
    task: Option<task::Task>,
    handle: SpawnHandle,
}

struct SinkWriteFuture<S: Sink, A> {
    inner: Rc<RefCell<InnerSinkWrite<S>>>,
    _actor: PhantomData<A>,
}

impl<S, A> ActorFuture for SinkWriteFuture<S, A>
where
    S: Sink,
    A: Actor + WriteHandler<S::SinkError>,
    A::Context: AsyncContext<A>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(
        &mut self,
        act: &mut A,
        ctxt: &mut A::Context,
    ) -> Poll<Self::Item, Self::Error> {
        let inner = &mut self.inner.borrow_mut();
        inner.task = None;

        if !inner.closing_flag.contains(Flags::CLOSING) {
            match inner.sink.poll_complete() {
                Err(e) => {
                    if act.error(e, ctxt) == Running::Stop {
                        act.finished(ctxt);
                        return Ok(Async::Ready(()));
                    }
                }
                Ok(Async::Ready(())) => {}
                Ok(Async::NotReady) => {}
            }
        } else {
            assert!(!inner.closing_flag.contains(Flags::CLOSED));
            match inner.sink.close() {
                Err(e) => {
                    if act.error(e, ctxt) == Running::Stop {
                        act.finished(ctxt);
                        return Ok(Async::Ready(()));
                    }
                }
                Ok(Async::Ready(())) => {
                    inner.closing_flag |= Flags::CLOSED;
                    act.finished(ctxt);
                    return Ok(Async::Ready(()));
                }
                Ok(Async::NotReady) => {}
            }
        }

        inner.task = Some(futures::task::current());
        Ok(Async::NotReady)
    }
}
