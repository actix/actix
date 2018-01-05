#![allow(dead_code)]

use std;
use std::collections::VecDeque;

use futures::{Async, AsyncSink, Future, Poll, Sink, Stream};
use futures::sync::oneshot::Sender as SyncSender;
use futures::unsync::oneshot::{channel, Sender as UnsyncSender, Receiver as UnsyncReceiver};
use tokio_core::reactor::Handle;
use tokio_io::AsyncRead;
use tokio_io::codec::{Framed, Encoder, Decoder};

use fut::ActorFuture;

use actor::{Actor, Supervised, SpawnHandle,
            FramedActor, ActorState, ActorContext, AsyncContext};
use address::{Subscriber};
use handler::{Handler, ResponseType, StreamHandler};
use context::{ActorAddressCell, ActorItemsCell, ActorWaitCell, AsyncContextApi};
use envelope::{Envelope, ToEnvelope, RemoteEnvelope};
use message::Response;


/// Actor execution context for
/// [Framed](https://docs.rs/tokio-io/0.1.3/tokio_io/codec/struct.Framed.html) object
pub struct FramedContext<A>
    where A: FramedActor + Actor<Context=FramedContext<A>>,
{
    act: A,
    state: ActorState,
    modified: bool,
    address: ActorAddressCell<A>,
    framed: Option<ActorFramedCell<A>>,
    wait: ActorWaitCell<A>,
    items: ActorItemsCell<A>,
}

impl<A> ToEnvelope<A> for FramedContext<A>
    where A: FramedActor + Actor<Context=FramedContext<A>>,
{
    fn pack<M>(msg: M, tx: Option<SyncSender<Result<M::Item, M::Error>>>) -> Envelope<A>
        where M: ResponseType + Send + 'static,
              A: Handler<M>,
              A: FramedActor + Actor<Context=FramedContext<A>>,
              M::Item: Send,
              M::Error: Send,
    {
        Envelope::new(RemoteEnvelope::new(msg, tx))
    }
}

impl<A> ActorContext for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    /// Stop actor execution
    ///
    /// This method closes actor address and framed object.
    fn stop(&mut self) {
        self.close();
        self.address.close();
        if self.state == ActorState::Running {
            self.state = ActorState::Stopping;
        }
    }

    /// Terminate actor execution
    fn terminate(&mut self) {
        self.close();
        self.address.close();
        self.items.close();
        self.state = ActorState::Stopped;
    }

    /// Actor execution state
    fn state(&self) -> ActorState {
        self.state
    }
}

impl<A> AsyncContext<A> for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.modified = true;
        self.items.spawn(fut)
    }

    fn wait<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.modified = true;
        self.wait.add(fut)
    }

    fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.modified = true;
        self.items.cancel_future(handle)
    }

    fn cancel_future_on_stop(&mut self, handle: SpawnHandle) {
        self.items.cancel_future_on_stop(handle)
    }
}

impl<A> AsyncContextApi<A> for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    fn address_cell(&mut self) -> &mut ActorAddressCell<A> {
        &mut self.address
    }
}

impl<A> FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    /// Send item to sink. If sink is closed item returned as an error.
    pub fn send(&mut self, msg: <<A as FramedActor>::Codec as Encoder>::Item)
                -> Result<(), <<A as FramedActor>::Codec as Encoder>::Item>
    {
        if let Some(ref mut framed) = self.framed {
            framed.send(msg);
            Ok(())
        } else {
            Err(msg)
        }
    }

    /// Gracefully close Framed object. FramedContext
    /// will try to send all buffered items and then close.
    /// FramedContext::stop() could be used to force stop sending process.
    pub fn close(&mut self) {
        self.items.stop();
        if let Some(ref mut framed) = self.framed.take() {
            framed.close();
        }
    }

    /// Initiate sink drain
    ///
    /// Returns oneshot future. It resolves when sink is drained.
    /// All other actor activities are paused.
    pub fn drain(&mut self) -> UnsyncReceiver<()> {
        if let Some(ref mut framed) = self.framed {
            framed.drain()
        } else {
            let (tx, rx) = channel();
            let _ = tx.send(());
            rx
        }
    }

    /// Get inner framed object
    pub fn take_framed(&mut self)
                       -> Option<Framed<<A as FramedActor>::Io, <A as FramedActor>::Codec>> {
        if let Some(cell) = self.framed.take() {
            Some(cell.into_framed())
        } else {
            None
        }
    }
}

impl<A> FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    #[doc(hidden)]
    pub fn subscriber<M>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>,
              M: ResponseType + 'static,
    {
        Box::new(self.address.unsync_address())
    }

    #[doc(hidden)]
    pub fn sync_subscriber<M>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              M: ResponseType + Send + 'static,
              M::Item: Send,
              M::Error: Send,
    {
        Box::new(self.address.sync_address())
    }
}

impl<A> FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    pub(crate) fn new(act: A, io: <A as FramedActor>::Io,
                      codec: <A as FramedActor>::Codec) -> FramedContext<A> {
        FramedContext {
            act: act,
            state: ActorState::Started,
            modified: false,
            address: ActorAddressCell::default(),
            framed: Some(ActorFramedCell::new(io.framed(codec))),
            wait: ActorWaitCell::default(),
            items: ActorItemsCell::default(),
        }
    }

    pub(crate) fn framed(act: A, framed: Framed<<A as FramedActor>::Io, <A as FramedActor>::Codec>)
                         -> FramedContext<A> {
        FramedContext {
            act: act,
            state: ActorState::Started,
            modified: false,
            address: ActorAddressCell::default(),
            framed: Some(ActorFramedCell::new(framed)),
            wait: ActorWaitCell::default(),
            items: ActorItemsCell::default(),
        }
    }

    pub(crate) fn run(self, handle: &Handle) {
        handle.spawn(self.map(|_| ()).map_err(|_| ()));
    }

    pub(crate) fn restarting(&mut self) where A: Supervised {
        let ctx: &mut FramedContext<A> = unsafe {
            std::mem::transmute(self as &mut FramedContext<A>)
        };
        self.act.restarting(ctx);
    }

    pub(crate) fn replace_actor(&mut self, srv: A) -> A {
        std::mem::replace(&mut self.act, srv)
    }

    pub(crate) fn address_cell(&mut self) -> &mut ActorAddressCell<A> {
        &mut self.address
    }

    pub(crate) fn into_inner(self) -> A {
        self.act
    }
}

#[doc(hidden)]
impl<A> Future for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ctx: &mut FramedContext<A> = unsafe {
            std::mem::transmute(self as &mut FramedContext<A>)
        };

        // update state
        match self.state {
            ActorState::Started => {
                Actor::started(&mut self.act, ctx);
                self.state = ActorState::Running;
            },
            ActorState::Stopping => {
                Actor::stopping(&mut self.act, ctx);
            }
            _ => ()
        }

        let mut prep_stop = false;
        loop {
            self.modified = false;

            // check wait futures
            if self.wait.poll(&mut self.act, ctx) {
                return Ok(Async::NotReady)
            }

            // framed
            let closed = if let Some(ref mut framed) = self.framed {
                // framed sink may need to drain
                if framed.poll(&mut self.act, ctx) {
                    return Ok(Async::NotReady)
                }
                framed.closed()
            } else {
                false
            };
            if closed {
                self.modified = true;
                self.framed.take();
                self.items.stop();
            }

            // incoming messages
            self.address.poll(&mut self.act, ctx);

            // check secondary streams
            self.items.poll(&mut self.act, ctx);

            // are we done
            if self.modified {
                continue
            }

            // check state
            match self.state {
                ActorState::Stopped => {
                    self.state = ActorState::Stopped;
                    Actor::stopped(&mut self.act, ctx);
                    return Ok(Async::Ready(()))
                },
                ActorState::Stopping => {
                    if prep_stop {
                        if self.framed.is_some() ||
                            self.address.connected() ||
                            !self.items.is_empty()
                        {
                            self.state = ActorState::Running;
                            continue
                        } else {
                            self.state = ActorState::Stopped;
                            Actor::stopped(&mut self.act, ctx);
                            return Ok(Async::Ready(()))
                        }
                    } else {
                        Actor::stopping(&mut self.act, ctx);
                        prep_stop = true;
                        continue
                    }
                },
                ActorState::Running => {
                    if !self.framed.is_some() && !self.address.connected() &&
                        self.items.is_empty()
                    {
                        self.state = ActorState::Stopping;
                        Actor::stopping(&mut self.act, ctx);
                        prep_stop = true;
                        continue
                    }
                },
                _ => (),
            }

            return Ok(Async::NotReady)
        }
    }
}

impl<A> std::fmt::Debug for FramedContext<A>
    where A: Actor<Context=Self> + FramedActor,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "FramedContext({:?}: actor:{:?}) {{ state: {:?}, connected: {}, items: {} }}",
               self as *const _,
               &self.act as *const _,
               self.state, "-", self.items.is_empty())
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
pub(crate)
struct ActorFramedCell<A>
    where A: Actor + FramedActor,
          A::Context: AsyncContext<A>,
{
    flags: FramedFlags,
    framed: Framed<<A as FramedActor>::Io, <A as FramedActor>::Codec>,
    sink_items: VecDeque<<<A as FramedActor>::Codec as Encoder>::Item>,
    drain: Option<UnsyncSender<()>>,
}

impl<A> ActorFramedCell<A>
    where A: Actor + FramedActor,
          A::Context: AsyncContext<A>,
{
    pub fn new(framed: Framed<<A as FramedActor>::Io, <A as FramedActor>::Codec>)
               -> ActorFramedCell<A>
    {
        ActorFramedCell {
            flags: FramedFlags::SINK_FLUSHED,
            framed: framed,
            sink_items: VecDeque::new(),
            drain: None,
        }
    }

    pub fn alive(&self) -> bool {
        !self.flags.contains(FramedFlags::SINK_CLOSED) &&
            !self.flags.contains(FramedFlags::STREAM_CLOSED)
    }

    pub fn close(&mut self) {
        self.flags.insert(FramedFlags::CLOSING);
    }

    pub fn closed(&mut self) -> bool {
        self.flags.contains(FramedFlags::STREAM_CLOSED | FramedFlags::SINK_CLOSED)
    }

    pub fn send(&mut self, msg: <<A as FramedActor>::Codec as Encoder>::Item) {
        self.sink_items.push_back(msg);
    }

    pub fn drain(&mut self) -> UnsyncReceiver<()> {
        if let Some(tx) = self.drain.take() {
            let _ = tx.send(());
            error!("drain method should be called once");
        }
        let (tx, rx) = channel();
        self.drain = Some(tx);
        rx
    }

    pub fn into_framed(self) -> Framed<<A as FramedActor>::Io, <A as FramedActor>::Codec> {
        self.framed
    }

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context) -> bool {
        loop {
            let mut not_ready = true;

            // framed stream
            if self.drain.is_none() && !self.flags.contains(FramedFlags::CLOSING) &&
                !self.flags.contains(FramedFlags::STREAM_CLOSED)
            {
                match self.framed.poll() {
                    Ok(Async::Ready(Some(msg))) => {
                        not_ready = false;
                        <A as FramedActor>::handle(act, Ok(msg), ctx);
                    }
                    Ok(Async::Ready(None)) => {
                        self.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                    }
                    Ok(Async::NotReady) => (),
                    Err(err) => {
                        self.flags |= FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                        <A as FramedActor>::handle(act, Err(err), ctx);
                    }
                }
            }

            if !self.flags.contains(FramedFlags::SINK_CLOSED) {
                // send sink items
                loop {
                    if let Some(msg) = self.sink_items.pop_front() {
                        match self.framed.start_send(msg) {
                            Ok(AsyncSink::NotReady(msg)) => {
                                self.sink_items.push_front(msg);
                                return self.drain.is_some();
                            }
                            Ok(AsyncSink::Ready) => {
                                self.flags.remove(FramedFlags::SINK_FLUSHED);
                                continue
                            }
                            Err(err) => {
                                self.flags |=
                                    FramedFlags::SINK_CLOSED | FramedFlags::STREAM_CLOSED;
                                <A as FramedActor>::error(act, err, ctx);
                                break
                            }
                        }
                    }
                    break
                }

                // flush sink
                if !self.flags.contains(FramedFlags::SINK_FLUSHED) {
                    match self.framed.poll_complete() {
                        Ok(Async::Ready(_)) => {
                            not_ready = false;
                            self.flags |= FramedFlags::SINK_FLUSHED;
                        }
                        Ok(Async::NotReady) =>
                            if self.drain.is_some() {
                                return true
                            },
                        Err(err) => {
                            self.flags |= FramedFlags::SINK_CLOSED | FramedFlags::SINK_FLUSHED;
                            <A as FramedActor>::error(act, err, ctx);
                        }
                    }
                }

                if self.flags.contains(FramedFlags::CLOSING) &&
                    self.flags.contains(FramedFlags::SINK_FLUSHED) && self.sink_items.is_empty() {
                        self.flags |= FramedFlags::SINK_CLOSED;
                    }
            }
            if let Some(tx) = self.drain.take() {
                let _ = tx.send(());
            }

            // are we done
            if !not_ready {
                continue
            }

            return false
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
        write: BytesMut,
        err: Option<io::Error>,
        write_err: Option<io::Error>,
        write_block: bool,
    }

    impl Buffer {
        fn new(data: &'static str) -> Buffer {
            Buffer {
                buf: Bytes::from(data),
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
    }

    impl Actor for TestActor {
        type Context = FramedContext<Self>;
    }

    impl FramedActor for TestActor {
        type Io = Buffer;
        type Codec = TestCodec;

        fn handle(&mut self, msg: Result<Item, io::Error>, _ctx: &mut Self::Context) {
            if let Ok(item) = msg {
                self.msgs.push(item.0);
            }
        }
    }

    #[test]
    fn test_basic() {
        let mut ctx = FramedContext::new(
            TestActor{msgs: Vec::new()}, Buffer::new(""), TestCodec);

        let _ = ctx.poll();
        ctx.framed.as_mut().unwrap().framed.get_mut().feed_data("data");

        // messages recevied
        let _ = ctx.poll();
        assert_eq!(ctx.act.msgs[0], b"da"[..]);
        assert_eq!(ctx.act.msgs[1], b"ta"[..]);

        // block sink
        ctx.framed.as_mut().unwrap().framed.get_mut().write_block = true;
        ctx.send(Bytes::from_static(b"11")).ok().unwrap();
        ctx.send(Bytes::from_static(b"22")).ok().unwrap();

        // drain
        let _ = ctx.drain();

        // new data in framed, actor is paused
        ctx.framed.as_mut().unwrap().framed.get_mut().feed_data("bb");
        let _ = ctx.poll();
        assert_eq!(ctx.act.msgs.len(), 2);

        // sink unblocked
        ctx.framed.as_mut().unwrap().framed.get_mut().write_block = false;
        let _ = ctx.poll();
        assert_eq!(ctx.act.msgs.len(), 3);
        assert_eq!(ctx.act.msgs[2], b"bb"[..]);

        // sink data
        assert_eq!(ctx.framed.as_mut().unwrap().framed.get_mut().write, b"1122"[..]);

        // println!("TEST: {:?}", ctx.act.msgs);
    }
}
