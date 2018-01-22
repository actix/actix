#![allow(dead_code)]

use std::{mem, fmt};
use std::rc::Rc;
use std::cell::UnsafeCell;
use std::collections::VecDeque;

use futures::{Async, AsyncSink, Future, Poll, Sink, Stream};
use futures::sync::oneshot::Sender as SyncSender;
use futures::unsync::oneshot::{channel, Sender as UnsyncSender, Receiver as UnsyncReceiver};
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::{Framed, Encoder};

use fut::ActorFuture;
use queue::unsync;

use actor::{Actor, Supervised, SpawnHandle,
            FramedActor, ActorState, ActorContext, AsyncContext};
use address::{Address, SyncAddress, Subscriber};
use handler::{Handler, ResponseType};
use context::{AsyncContextApi, ContextProtocol};
use contextimpl::ContextImpl;
use envelope::{Envelope, ToEnvelope, RemoteEnvelope};


/// Actor execution context for
/// [Framed](https://docs.rs/tokio-io/0.1.3/tokio_io/codec/struct.Framed.html) object
pub struct FramedContext<A>
    where A: FramedActor + Actor<Context=FramedContext<A>>,
{
    inner: ContextImpl<A>,
    framed: FramedCell<A>,
}

impl<A> ToEnvelope<A> for FramedContext<A>
    where A: FramedActor + Actor<Context=FramedContext<A>>,
{
    #[inline]
    fn pack<M>(msg: M,
               tx: Option<SyncSender<Result<M::Item, M::Error>>>,
               cancel_on_drop: bool) -> Envelope<A>
        where M: ResponseType + Send + 'static,
              A: Handler<M> + FramedActor + Actor<Context=FramedContext<A>>,
              M::Item: Send, M::Error: Send {
        Envelope::new(RemoteEnvelope::new(msg, tx, cancel_on_drop))
    }
}

impl<A> ActorContext for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    #[inline]
    fn stop(&mut self) {
        self.inner.stop()
    }
    #[inline]
    fn terminate(&mut self) {
        self.inner.terminate()
    }
    #[inline]
    fn state(&self) -> ActorState {
        self.inner.state()
    }
}

impl<A> AsyncContext<A> for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    #[inline]
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.inner.spawn(fut)
    }
    #[inline]
    fn wait<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.inner.wait(fut)
    }
    #[doc(hidden)]
    #[inline]
    fn waiting(&self) -> bool {
        self.inner.waiting()
    }
    #[inline]
    fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.inner.cancel_future(handle)
    }
}

#[doc(hidden)]
impl<A> AsyncContextApi<A> for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    #[inline]
    fn unsync_sender(&mut self) -> unsync::UnboundedSender<ContextProtocol<A>> {
        self.inner.unsync_sender()
    }
    #[inline]
    fn unsync_address(&mut self) -> Address<A> {
        self.inner.unsync_address()
    }
    #[inline]
    fn sync_address(&mut self) -> SyncAddress<A> {
        self.inner.sync_address()
    }
}

impl<A> FramedContext<A> where A: Actor<Context=Self> + FramedActor
{
    /// Send item to sink. If sink is closed item returned as an error.
    #[inline]
    pub fn send(&mut self, msg: <A::Codec as Encoder>::Item)
                -> Result<(), <A::Codec as Encoder>::Item> {
        self.framed.send(msg);
        Ok(())
    }

    /// Gracefully close Framed object. FramedContext
    /// will try to send all buffered items and then close.
    /// FramedContext::stop() could be used to force stop sending process.
    #[inline]
    pub fn close(&mut self) {
        self.framed.close()
    }

    /// Initiate sink drain
    ///
    /// Returns oneshot future. It resolves when sink is drained.
    /// All other actor activities are paused.
    #[inline]
    pub fn drain(&mut self) -> UnsyncReceiver<()> {
        self.framed.drain()
    }

    /// Get inner framed object
    #[inline]
    pub fn take(&mut self) -> Option<Framed<A::Io, A::Codec>> {
        self.framed.take()
    }

    /// Replace existing framed object with new object.
    ///
    /// Consider to use `drain()` before replace framed object,
    /// because Sink buffer get dropped as well.
    pub fn replace(&mut self, framed: Framed<A::Io, A::Codec>) {
        self.framed.close();
        let (wrp, cell) = FramedWrapper::new(framed);
        self.framed = cell;
        self.inner.spawn(wrp);
    }
}

#[doc(hidden)]
impl<A> FramedContext<A> where A: Actor<Context=Self> + FramedActor
{
    #[inline]
    pub fn subscriber<M>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>, M: ResponseType + 'static {
        self.inner.subscriber()
    }
    #[inline]
    pub fn sync_subscriber<M>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send {
        self.inner.sync_subscriber()
    }
}

impl<A> FramedContext<A> where A: Actor<Context=Self> + FramedActor
{
    #[inline]
    pub(crate) fn new(act: Option<A>, io: A::Io, codec: A::Codec) -> FramedContext<A> {
        FramedContext::framed(act, io.framed(codec))
    }

    #[inline]
    pub(crate) fn framed(act: Option<A>, framed: Framed<A::Io, A::Codec>) -> FramedContext<A> {
        let (wrp, cell) = FramedWrapper::new(framed);
        let mut imp = ContextImpl::new(act);
        imp.spawn(wrp);

        FramedContext {inner: imp, framed: cell}
    }

    #[inline]
    pub(crate) fn run(self, handle: &Handle) {
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
    }

    #[inline]
    pub(crate) fn restarting(&mut self) where A: Supervised {
        let ctx: &mut FramedContext<A> = unsafe {
            mem::transmute(self as &mut FramedContext<A>)
        };
        self.inner.actor().restarting(ctx);
    }

    #[inline]
    pub(crate) fn set_actor(&mut self, act: A) {
        self.inner.set_actor(act)
    }

    #[inline]
    pub(crate) fn into_inner(self) -> A {
        self.inner.into_inner().unwrap()
    }
}

#[doc(hidden)]
impl<A> Future for FramedContext<A> where A: Actor<Context=Self> + FramedActor {
    type Item = ();
    type Error = ();

    #[inline]
    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut FramedContext<A> = unsafe {
            mem::transmute(self as &mut FramedContext<A>)
        };
        self.inner.poll(ctx)
    }
}

impl<A> fmt::Debug for FramedContext<A> where A: Actor<Context=Self> + FramedActor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FramedContext({:?})", self as *const _)
    }
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

/// Framed object wrapper
pub(crate) struct FramedWrapper<A> where A: Actor + FramedActor {
    inner: Rc<UnsafeCell<InnerActorFramedCell<A>>>,
}

/// Wrapper for a framed object
///
/// `FramedCell` instance can be used only within same context.
pub struct FramedCell<A> where A: Actor + FramedActor {
    inner: Rc<UnsafeCell<InnerActorFramedCell<A>>>,
}

struct InnerActorFramedCell<A> where A: Actor + FramedActor {
    flags: FramedFlags,
    framed: Option<Framed<A::Io, A::Codec>>,
    sink_items: VecDeque<<A::Codec as Encoder>::Item>,
    drain: Option<UnsyncSender<()>>,
    error: Option<<A::Codec as Encoder>::Error>,
}

impl<A> FramedWrapper<A> where A: Actor + FramedActor {

    pub fn new(framed: Framed<A::Io, A::Codec>) -> (FramedWrapper<A>, FramedCell<A>) {
        let inner = Rc::new(UnsafeCell::new(
            InnerActorFramedCell {
                flags: FramedFlags::SINK_FLUSHED,
                framed: Some(framed),
                sink_items: VecDeque::new(),
                drain: None,
                error: None,
            }));

        (FramedWrapper{inner: Rc::clone(&inner)}, FramedCell{inner: inner})
    }
}

impl<A> FramedCell<A> where A: Actor + FramedActor {

    #[inline]
    fn as_ref(&self) -> &InnerActorFramedCell<A> {
        unsafe{ &*self.inner.get() }
    }

    #[inline]
    fn as_mut(&mut self) -> &mut InnerActorFramedCell<A> {
        unsafe{ &mut *self.inner.get() }
    }

    #[inline]
    pub fn framed(&mut self) -> &mut Framed<A::Io, A::Codec> {
        self.as_mut().framed.as_mut().unwrap()
    }

    /// Close frame object
    pub fn close(&mut self) {
        self.as_mut().flags.insert(FramedFlags::CLOSING);
    }

    /// Check if framed object is closed
    pub fn closed(&self) -> bool {
        self.as_ref().flags.contains(FramedFlags::STREAM_CLOSED | FramedFlags::SINK_CLOSED)
    }

    /// Send item to a sink.
    pub fn send(&mut self, msg: <<A as FramedActor>::Codec as Encoder>::Item) {
        let inner = self.as_mut();

        // try to write to sink immediately
        if inner.sink_items.is_empty() && inner.framed.is_none() {
            inner.flags.remove(FramedFlags::SINK_FLUSHED);
            match inner.framed.as_mut().unwrap().start_send(msg) {
                Ok(AsyncSink::NotReady(msg)) => inner.sink_items.push_front(msg),
                Ok(AsyncSink::Ready) => (),
                Err(err) => inner.error = Some(err),
            }
        } else {
            inner.sink_items.push_back(msg);
        }
    }

    /// Initiate sink drain
    ///
    /// Returns oneshot future. It resolves when sink is drained.
    /// All other actor activities are paused.
    pub fn drain(&mut self) -> UnsyncReceiver<()> {
        let inner = self.as_mut();

        if let Some(tx) = inner.drain.take() {
            let _ = tx.send(());
            error!("drain method should be called once");
        }
        let (tx, rx) = channel();
        inner.drain = Some(tx);
        rx
    }

    /// Get inner framed object
    pub fn take(&mut self) -> Option<Framed<A::Io, A::Codec>> {
        self.as_mut().framed.take()
    }
}

impl<A> ActorFuture for FramedWrapper<A>
    where A: Actor + FramedActor, A::Context: AsyncContext<A> + AsyncContextApi<A>,
{
    type Item = ();
    type Error = ();
    type Actor = A;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> Poll<Self::Item, Self::Error> {
        let inner = unsafe{ &mut *self.inner.get() };

        if let Some(err) = inner.error.take() {
            inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
            <A as FramedActor>::closed(act, Some(err), ctx);
        }

        let framed: &mut Framed<A::Io, A::Codec> = if let Some(ref mut framed) = inner.framed {
            unsafe { mem::transmute(framed) }
        } else {
            return Ok(Async::Ready(()));
        };

        loop {
            let mut not_ready = true;

            // check framed stream
            if inner.drain.is_none() && !inner.flags.intersects(
                FramedFlags::CLOSING | FramedFlags::STREAM_CLOSED)
            {
                match framed.poll() {
                    Ok(Async::Ready(Some(msg))) => {
                        not_ready = false;
                        <A as FramedActor>::handle(act, Ok(msg), ctx);
                    }
                    Ok(Async::Ready(None)) => {
                        inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                        <A as FramedActor>::closed(act, None, ctx);
                    }
                    Ok(Async::NotReady) => (),
                    Err(err) => {
                        inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                        <A as FramedActor>::handle(act, Err(err), ctx);
                    }
                }
            }

            if !inner.flags.contains(FramedFlags::SINK_CLOSED) {
                // send sink items
                loop {
                    if let Some(msg) = inner.sink_items.pop_front() {
                        inner.flags.remove(FramedFlags::SINK_FLUSHED);
                        match framed.start_send(msg) {
                            Ok(AsyncSink::NotReady(msg)) => {
                                inner.sink_items.push_front(msg);
                                if inner.drain.is_some() {
                                    return Ok(Async::NotReady)
                                }
                                break
                            }
                            Ok(AsyncSink::Ready) => {
                                continue
                            }
                            Err(err) => {
                                inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                                <A as FramedActor>::closed(act, Some(err), ctx);
                                break
                            }
                        }
                    }
                    break
                }

                // flush sink
                if !inner.flags.contains(FramedFlags::SINK_FLUSHED) {
                    match framed.poll_complete() {
                        Ok(Async::Ready(_)) => {
                            not_ready = false;
                            inner.flags |= FramedFlags::SINK_FLUSHED;
                        }
                        Ok(Async::NotReady) =>
                            if inner.drain.is_some() {
                                return Ok(Async::NotReady);
                            },
                        Err(err) => {
                            inner.flags |= FramedFlags::SINK_CLOSED | FramedFlags::SINK_FLUSHED;
                            <A as FramedActor>::closed(act, Some(err), ctx);
                        }
                    }
                }

                // close framed object, if closing and we dont need to flush any data
                if inner.flags.contains(FramedFlags::CLOSING | FramedFlags::SINK_FLUSHED) &&
                    inner.sink_items.is_empty() {
                        inner.flags |= FramedFlags::SINK_CLOSED;
                    }
            }
            if let Some(tx) = inner.drain.take() {
                let _ = tx.send(());
            }

            // are we done
            if not_ready {
                if inner.flags.contains(
                    FramedFlags::STREAM_CLOSED | FramedFlags::SINK_CLOSED)
                {
                    ctx.stop();
                    return Ok(Async::Ready(()))
                } else if ctx.state() == ActorState::Stopping {
                    let _ = framed.get_mut().shutdown();
                }
                return Ok(Async::NotReady)
            }
            if ctx.waiting() {
                return Ok(Async::NotReady)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cmp, io};
    use bytes::{Bytes, BytesMut};
    use tokio_io::AsyncWrite;
    use tokio_io::codec::{Encoder, Decoder};

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
        type Context = FramedContext<Self>;
    }

    impl FramedActor for TestActor {
        type Io = Buffer;
        type Codec = TestCodec;

        fn handle(&mut self, msg: Result<Item, io::Error>, _: &mut Self::Context) {
            if let Ok(item) = msg {
                self.msgs.push(item.0);
            }
        }

        fn closed(&mut self, err: Option<io::Error>, _: &mut Self::Context) {
            self.error = err;
            self.closed = true;
        }
    }

    #[test]
    fn test_basic() {
        let mut ctx = FramedContext::new(
            Some(TestActor::new()), Buffer::new(""), TestCodec);

        let _ = ctx.poll();
        ctx.framed.as_mut().framed.as_mut().unwrap().get_mut().feed_data("data");

        // messages received
        let _ = ctx.poll();
        assert_eq!(ctx.inner.actor().msgs[0], b"da"[..]);
        assert_eq!(ctx.inner.actor().msgs[1], b"ta"[..]);

        // block sink
        ctx.framed.as_mut().framed.as_mut().unwrap().get_mut().write_block = true;
        let _ = ctx.send(Bytes::from_static(b"11"));
        let _ = ctx.send(Bytes::from_static(b"22"));

        // drain
        let _ = ctx.drain();

        // new data in framed, actor is paused
        ctx.framed.as_mut().framed.as_mut().unwrap().get_mut().feed_data("bb");
        let _ = ctx.poll();
        assert_eq!(ctx.inner.actor().msgs.len(), 2);

        // sink unblocked
        ctx.framed.as_mut().framed.as_mut().unwrap().get_mut().write_block = false;
        let _ = ctx.poll();
        assert_eq!(ctx.inner.actor().msgs.len(), 3);
        assert_eq!(ctx.inner.actor().msgs[2], b"bb"[..]);

        // sink data
        assert_eq!(ctx.framed.as_mut().framed.as_mut().unwrap().get_mut().write, b"1122"[..]);
    }

    #[test]
    fn test_multiple_message() {
        let mut ctx = FramedContext::new(
            Some(TestActor::new()), Buffer::new(""), TestCodec);

        let _ = ctx.poll();
        ctx.framed.as_mut().framed.as_mut().unwrap().get_mut().feed_data("11223344");

        // messages received
        let _ = ctx.poll();
        assert_eq!(ctx.inner.actor().msgs,
                   vec![Bytes::from_static(b"11"), Bytes::from_static(b"22"),
                        Bytes::from_static(b"33"), Bytes::from_static(b"44")]);
    }

    #[test]
    fn test_error_during_poll() {
        let mut ctx = FramedContext::new(
            Some(TestActor::new()), Buffer::new(""), TestCodec);

        let _ = ctx.poll();
        ctx.framed.as_mut().framed.as_mut().unwrap().get_mut().write_err =
            Some(io::Error::new(io::ErrorKind::Other, "error"));

        ctx.framed.as_mut().sink_items.push_back(Bytes::from_static(b"11"));
        let _ = ctx.poll();
        assert!(ctx.inner.actor().error.is_some());
        assert!(ctx.inner.actor().closed);
    }

    #[test]
    fn test_close() {
        let mut buf = Buffer::new("");
        buf.eof = true;
        let mut ctx = FramedContext::new(Some(TestActor::new()), buf, TestCodec);

        let _ = ctx.poll();
        assert!(ctx.inner.actor().error.is_none());
        assert!(ctx.inner.actor().closed);
        assert!(ctx.framed.closed());
    }
}
