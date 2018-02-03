use std::mem;
use std::rc::Rc;
use std::marker::PhantomData;
use std::cell::UnsafeCell;
use std::collections::VecDeque;

use futures::{Async, AsyncSink, Poll, Sink, Stream};
use futures::unsync::oneshot::{channel, Sender as UnsyncSender};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::{Framed, Encoder, Decoder};

use fut::ActorFuture;
use actor::{Actor, AsyncContext};
use utils::Drain;
use stream::StreamHandler;

pub enum FramedError<Codec: Encoder + Decoder> {
    StreamError(<Codec as Decoder>::Error),
    SinkError(<Codec as Encoder>::Error),
}

bitflags! {
    struct FramedFlags: u8 {
        const STARTED = 0b0000_0001;
        const CLOSING = 0b0000_0010;
        const STREAM_CLOSED = 0b0000_0100;
        const SINK_CLOSED = 0b0000_1000;
        const SINK_FLUSHED = 0b0001_0000;
    }
}

/// Framed type wrapper, that simplify handling framed objects with actor
pub struct FramedReader<Io, Codec>
    where Io: AsyncRead + AsyncWrite, Codec: Encoder + Decoder
{
    inner: Rc<UnsafeCell<InnerActorFramedCell<Io, Codec>>>,
}

/// Framed sink type wrapper, that simplify handling framed objects with actor
pub struct FramedWriter<Io, Codec>
    where Io: AsyncRead + AsyncWrite, Codec: Encoder + Decoder
{
    inner: Rc<UnsafeCell<InnerActorFramedCell<Io, Codec>>>,
}

pub(crate) struct FramedDrain<A, Io, Codec>
    where A: StreamHandler<<Codec as Decoder>::Item, FramedError<Codec>>,
          Io: AsyncRead + AsyncWrite, Codec: Encoder + Decoder,
{
    tx: Option<UnsyncSender<()>>,
    inner: Rc<UnsafeCell<InnerActorFramedCell<Io, Codec>>>,
    marker: PhantomData<A>,
}

struct InnerActorFramedCell<Io, Codec>
    where Io: AsyncRead + AsyncWrite, Codec: Encoder + Decoder
{
    flags: FramedFlags,
    framed: Option<Framed<Io, Codec>>,
    sink_items: VecDeque<<Codec as Encoder>::Item>,
    error: Option<<Codec as Encoder>::Error>,
}

impl<Io, Codec> FramedReader<Io, Codec>
    where Io: AsyncRead + AsyncWrite + 'static, Codec: Encoder + Decoder + 'static
{
    pub fn wrap(framed: Framed<Io, Codec>) -> (FramedReader<Io, Codec>, FramedWriter<Io, Codec>)
    {
        let inner = Rc::new(UnsafeCell::new(
            InnerActorFramedCell {
                flags: FramedFlags::SINK_FLUSHED,
                framed: Some(framed),
                sink_items: VecDeque::new(),
                error: None,
            }));

        (FramedReader{inner: Rc::clone(&inner)}, FramedWriter{inner: inner})
    }
}

impl<Io, Codec> FramedWriter<Io, Codec>
    where Io: AsyncRead + AsyncWrite + 'static, Codec: Encoder + Decoder + 'static
{
    #[inline]
    fn as_ref(&self) -> &InnerActorFramedCell<Io, Codec> {
        unsafe{ &*self.inner.get() }
    }

    #[inline]
    fn as_mut(&mut self) -> &mut InnerActorFramedCell<Io, Codec> {
        unsafe{ &mut *self.inner.get() }
    }

    /// Gracefully close framed object
    ///
    /// Close process is asynchronous.
    pub fn close(&mut self) {
        self.as_mut().flags.insert(FramedFlags::CLOSING);
    }

    /// Check if framed object is closed
    pub fn closed(&self) -> bool {
        self.as_ref().flags.contains(FramedFlags::STREAM_CLOSED | FramedFlags::SINK_CLOSED)
    }

    /// Send item to a sink.
    pub fn send(&mut self, msg: <Codec as Encoder>::Item) {
        let inner = self.as_mut();

        // try to write to sink immediately
        if inner.sink_items.is_empty() && inner.framed.is_some() {
            inner.flags.remove(FramedFlags::SINK_FLUSHED);
            match inner.framed.as_mut().unwrap().start_send(msg) {
                Ok(AsyncSink::NotReady(msg)) => inner.sink_items.push_back(msg),
                Ok(AsyncSink::Ready) => (),
                Err(err) => inner.error = Some(err),
            }
        } else {
            inner.sink_items.push_back(msg);
        }
    }

    /// Initiate sink drain
    ///
    /// Returns future. It resolves when sink is drained.
    /// All other actor activities are paused until framed object get drained.
    pub fn drain<A, T>(&mut self, ctx: &mut T) -> Drain
        where A: Actor<Context=T> + StreamHandler<<Codec as Decoder>::Item, FramedError<Codec>>,
              T: AsyncContext<A>
    {
        let (tx, rx) = channel();

        let drain = FramedDrain{tx: Some(tx),
                                inner: Rc::clone(&self.inner),
                                marker: PhantomData};
        ctx.wait(drain);

        Drain::new(rx)
    }

    /// Take inner framed object
    pub fn take(&mut self) -> Option<Framed<Io, Codec>> {
        self.as_mut().framed.take()
    }
}

impl<Io, Codec> Stream for FramedReader<Io, Codec>
    where Io: AsyncRead + AsyncWrite, Codec: Encoder + Decoder,
{
    type Item = <Codec as Decoder>::Item;
    type Error = FramedError<Codec>;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let inner = unsafe{ &mut *self.inner.get() };

        // error from `send()`
        if let Some(err) = inner.error.take() {
            inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
            return Err(FramedError::SinkError(err));
        }

        // get framed object, remove lifetime
        let framed: &mut Framed<Io, Codec> = if let Some(ref mut framed) = inner.framed {
            unsafe { mem::transmute(framed) }
        } else {
            return Ok(Async::Ready(None));
        };

        // check framed stream
        if !inner.flags.intersects(FramedFlags::CLOSING | FramedFlags::STREAM_CLOSED) {
            match framed.poll() {
                Ok(Async::NotReady) => (),
                Ok(Async::Ready(Some(msg))) => return Ok(Async::Ready(Some(msg))),
                Ok(Async::Ready(None)) => {
                    inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                }
                Err(err) => {
                    inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                    return Err(FramedError::StreamError(err));
                }
            }

            while !inner.flags.contains(FramedFlags::SINK_CLOSED) {
                // send sink items
                loop {
                    if let Some(msg) = inner.sink_items.pop_front() {
                        inner.flags.remove(FramedFlags::SINK_FLUSHED);
                        match framed.start_send(msg) {
                            Ok(AsyncSink::NotReady(msg)) => {
                                inner.sink_items.push_front(msg);
                                break
                            }
                            Ok(AsyncSink::Ready) => {
                                continue
                            }
                            Err(err) => {
                                inner.flags |=
                                    FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                                return Err(FramedError::SinkError(err));
                            }
                        }
                    }
                    break
                }

                // flush sink
                if !inner.flags.contains(FramedFlags::SINK_FLUSHED) {
                    match framed.poll_complete() {
                        Ok(Async::Ready(_)) => {
                            inner.flags |= FramedFlags::SINK_FLUSHED;
                            continue
                        }
                        Ok(Async::NotReady) => break,
                        Err(err) => {
                            inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::SINK_FLUSHED;
                            return Err(FramedError::SinkError(err));
                        }
                    }
                }
                break
            }

            // close framed object, if closing and we dont need to flush any data
            if inner.flags.contains(FramedFlags::CLOSING | FramedFlags::SINK_FLUSHED) &&
                inner.sink_items.is_empty() {
                    inner.flags |= FramedFlags::SINK_CLOSED;
                }

            // are we done
            if inner.flags.contains(FramedFlags::STREAM_CLOSED | FramedFlags::SINK_CLOSED) {
                Ok(Async::Ready(None))
            } else {
                Ok(Async::NotReady)
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl<A, Io, Codec> ActorFuture for FramedDrain<A, Io, Codec>
    where A: StreamHandler<<Codec as Decoder>::Item, FramedError<Codec>>,
          A::Context: AsyncContext<A>,
          Io: AsyncRead + AsyncWrite,
          Codec: Encoder + Decoder,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        let inner = unsafe{ &mut *self.inner.get() };
        let framed: &mut Framed<Io, Codec> = if let Some(ref mut framed) = inner.framed {
            unsafe { mem::transmute(framed) }
        } else {
            return Ok(Async::Ready(()));
        };

        if !inner.flags.contains(FramedFlags::SINK_CLOSED) {
            // send sink items
            loop {
                if let Some(msg) = inner.sink_items.pop_front() {
                    inner.flags.remove(FramedFlags::SINK_FLUSHED);
                    match framed.start_send(msg) {
                        Ok(AsyncSink::NotReady(msg)) => {
                            inner.sink_items.push_front(msg);
                            return Ok(Async::NotReady)
                        }
                        Ok(AsyncSink::Ready) => continue,
                        Err(err) => {
                            inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                            if A::error(act, FramedError::SinkError(err), ctx) {
                                A::finished(act, ctx);
                                return Ok(Async::Ready(()))
                            }
                            break
                        }
                    }
                }
                break
            }

            // flush sink
            if !inner.flags.contains(FramedFlags::SINK_FLUSHED) {
                match framed.poll_complete() {
                    Ok(Async::Ready(_)) => inner.flags |= FramedFlags::SINK_FLUSHED,
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(err) => {
                        inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::SINK_FLUSHED;
                        if A::error(act, FramedError::SinkError(err), ctx) {
                            A::finished(act, ctx);
                            return Ok(Async::Ready(()))
                        }
                    }
                }
            }

            // close framed object, if closing and we dont need to flush any data
            if inner.flags.contains(FramedFlags::CLOSING | FramedFlags::SINK_FLUSHED) &&
                inner.sink_items.is_empty() {
                    inner.flags |= FramedFlags::SINK_CLOSED;
                }
        }

        if let Some(tx) = self.tx.take() {
            let _ = tx.send(());
        }
        Ok(Async::Ready(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cmp, io};
    use bytes::{Bytes, BytesMut};
    use futures::Future;
    use tokio_io::{AsyncWrite, AsyncRead};
    use tokio_io::codec::{Encoder, Decoder};
    use prelude::*;

    struct Buffer {
        buf: Bytes,
        eof: bool,
        write: BytesMut,
        err: Option<io::Error>,
        write_err: Option<io::Error>,
        write_block: bool,
    }

    impl Buffer {
        fn new(data: &'static str) -> Buffer {
            Buffer {
                buf: Bytes::from(data),
                eof: false,
                write: BytesMut::new(),
                err: None,
                write_err: None,
                write_block: false,
            }
        }
        fn feed_data(&mut self, data: &'static str) {
            let mut b = BytesMut::from(self.buf.as_ref());
            b.extend(data.as_bytes());
            self.buf = b.take().freeze();
        }
    }

    impl AsyncRead for Buffer {}
    impl io::Read for Buffer {
        fn read(&mut self, dst: &mut [u8]) -> Result<usize, io::Error> {
            if self.buf.is_empty() {
                if self.err.is_some() {
                    Err(self.err.take().unwrap())
                } else if self.eof {
                    Ok(0)
                } else {
                    Err(io::Error::new(io::ErrorKind::WouldBlock, ""))
                }
            } else {
                let size = cmp::min(self.buf.len(), dst.len());
                let b = self.buf.split_to(size);
                dst[..size].copy_from_slice(&b);
                Ok(size)
            }
        }
    }

    impl io::Write for Buffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.write_block {
                Err(io::Error::new(io::ErrorKind::WouldBlock, ""))
            } else if let Some(err) = self.write_err.take() {
                Err(err)
            } else {
                self.write.extend(buf);
                Ok(buf.len())
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl AsyncWrite for Buffer {
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            Ok(Async::Ready(()))
        }
    }

    struct Item(Bytes);
    impl ResponseType for Item {
        type Item = ();
        type Error = ();
    }

    struct TestCodec;

    impl Decoder for TestCodec {
        type Item = Item;
        type Error = io::Error;

        fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
            if src.len() >= 2 {
                Ok(Some(Item(src.split_to(2).freeze())))
            } else {
                Ok(None)
            }
        }
    }

    impl Encoder for TestCodec {
        type Item = Bytes;
        type Error = io::Error;

        fn encode(&mut self, msg: Bytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
            dst.extend(msg);
            Ok(())
        }
    }

    struct TestActor {
        msgs: Vec<Bytes>,
        closed: bool,
        error: Option<io::Error>,
    }

    impl TestActor {
        fn new() -> TestActor {
            TestActor{msgs: Vec::new(), closed: false, error: None}
        }
    }

    impl Actor for TestActor {
        type Context = Context<Self>;
    }

    impl StreamHandler<Item, FramedError<TestCodec>> for TestActor {

        fn handle(&mut self, msg: Item, _: &mut Self::Context) {
            self.msgs.push(msg.0);
        }

        fn error(&mut self, err: FramedError<TestCodec>, _: &mut Self::Context) -> bool {
            match err {
                FramedError::StreamError(err) => self.error = Some(err),
                FramedError::SinkError(err) => self.error = Some(err),
            }
            true
        }

        fn finished(&mut self, _: &mut Self::Context) {
            self.closed = true;
        }
    }

    fn create_ctx(buf: Buffer) -> (Context<TestActor>, FramedWriter<Buffer, TestCodec>) {
        let act = TestActor::new();
        let mut ctx = Context::new(None);
        let (rx, tx) = FramedReader::wrap(buf.framed(TestCodec));
        TestActor::add_stream(rx, &mut ctx);
        ctx.set_actor(act);
        (ctx, tx)
    }

    #[test]
    fn test_basic() {
        let (mut ctx, mut cell) = create_ctx(Buffer::new(""));

        let _ = ctx.poll();
        cell.as_mut().framed.as_mut().unwrap().get_mut().feed_data("data");

        // messages received
        let _ = ctx.poll();
        assert_eq!(ctx.actor().msgs[0], b"da"[..]);
        assert_eq!(ctx.actor().msgs[1], b"ta"[..]);

        // block sink
        cell.as_mut().framed.as_mut().unwrap().get_mut().write_block = true;
        cell.send(Bytes::from_static(b"11"));
        cell.send(Bytes::from_static(b"22"));

        // drain
        let _ = cell.drain(&mut ctx);

        // new data in framed, actor is paused
        cell.as_mut().framed.as_mut().unwrap().get_mut().feed_data("bb");
        let _ = ctx.poll();
        assert_eq!(ctx.actor().msgs.len(), 2);

        // sink unblocked
        cell.as_mut().framed.as_mut().unwrap().get_mut().write_block = false;
        let _ = ctx.poll();
        assert_eq!(ctx.actor().msgs.len(), 3);
        assert_eq!(ctx.actor().msgs[2], b"bb"[..]);

        // sink data
        assert_eq!(cell.as_mut().framed.as_mut().unwrap().get_mut().write, b"1122"[..]);
    }

    #[test]
    fn test_multiple_message() {
        let (mut ctx, mut cell) = create_ctx(Buffer::new(""));

        let _ = ctx.poll();
        cell.as_mut().framed.as_mut().unwrap().get_mut().feed_data("11223344");

        // messages received
        let _ = ctx.poll();
        assert_eq!(ctx.actor().msgs,
                   vec![Bytes::from_static(b"11"), Bytes::from_static(b"22"),
                        Bytes::from_static(b"33"), Bytes::from_static(b"44")]);
    }

    #[test]
    fn test_drain() {
        let (mut ctx, mut cell) = create_ctx(Buffer::new(""));

        let _ = ctx.poll();

        // block sink
        cell.as_mut().framed.as_mut().unwrap().get_mut().write_block = true;
        cell.as_mut().sink_items.push_back(Bytes::from_static(b"11"));
        cell.send(Bytes::from_static(b"22"));

        // drain
        let _ = cell.drain(&mut ctx);

        // new data in framed, actor is paused
        cell.as_mut().framed.as_mut().unwrap().get_mut().feed_data("bb");
        let _ = ctx.poll();
        assert_eq!(ctx.actor().msgs.len(), 0);

        // sink unblocked
        cell.as_mut().framed.as_mut().unwrap().get_mut().write_block = false;
        let _ = ctx.poll();
        assert_eq!(ctx.actor().msgs.len(), 1);
        assert_eq!(ctx.actor().msgs[0], b"bb"[..]);

        // sink data
        assert_eq!(cell.as_mut().framed.as_mut().unwrap().get_mut().write, b"1122"[..]);
    }

    #[test]
    fn test_error_during_poll() {
        let (mut ctx, mut cell) = create_ctx(Buffer::new(""));

        let _ = ctx.poll();
        cell.as_mut().framed.as_mut().unwrap().get_mut().write_err =
            Some(io::Error::new(io::ErrorKind::Other, "error"));

        cell.as_mut().sink_items.push_back(Bytes::from_static(b"11"));
        let _ = ctx.poll();
        assert!(ctx.actor().error.is_some());
        assert!(ctx.actor().closed);
    }

    #[test]
    fn test_close() {
        let mut buf = Buffer::new("");
        buf.eof = true;
        let (mut ctx, cell) = create_ctx(buf);

        let _ = ctx.poll();
        assert!(ctx.actor().error.is_none());
        assert!(ctx.actor().closed);
        assert!(cell.closed());
    }
}
