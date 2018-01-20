use std::marker::PhantomData;

use futures::{Async, Stream};
use futures::unsync::oneshot::Sender;

use fut::ActorFuture;
use queue::{sync, unsync};

use actor::{Actor, AsyncContext};
use address::{Address, SyncAddress};
use envelope::Envelope;
use constants::MAX_SYNC_POLLS;


#[derive(Debug, PartialEq)]
pub enum ContextCellResult {
    NotReady,
    Ready,
    Stop,
    Completed,
}

/// context protocol
pub enum ContextProtocol<A: Actor> {
    /// message envelope
    Envelope(Envelope<A>),
    /// Request sync address
    Upgrade(Sender<SyncAddress<A>>),
}

pub trait ContextCell<A> where Self: 'static, A: Actor {
    /// Poll cell
    fn poll(&mut self, act: &mut A, ctx: &mut A::Context, stop: bool) -> ContextCellResult;
}

pub struct ActorAddressCell<A> where A: Actor {
    sync_msgs: Option<sync::UnboundedReceiver<Envelope<A>>>,
    unsync_msgs: unsync::UnboundedReceiver<ContextProtocol<A>>,
}

impl<A> Default for ActorAddressCell<A> where A: Actor {

    #[inline]
    fn default() -> Self {
        ActorAddressCell {
            sync_msgs: None,
            unsync_msgs: unsync::unbounded() }
    }
}

struct NumPolls(u32);

impl NumPolls {
    fn inc(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

impl<A> ActorAddressCell<A> where A: Actor, A::Context: AsyncContext<A>
{
    #[inline]
    pub fn new(rx: sync::UnboundedReceiver<Envelope<A>>) -> Self {
        ActorAddressCell {
            sync_msgs: Some(rx),
            unsync_msgs: unsync::unbounded() }
    }

    #[inline]
    pub fn close(&mut self) {
        self.unsync_msgs.close();
        if let Some(ref mut msgs) = self.sync_msgs {
            msgs.close()
        }
    }

    #[inline]
    pub fn connected(&mut self) -> bool {
        self.unsync_msgs.connected() ||
            self.sync_msgs.as_ref().map(|msgs| msgs.connected()).unwrap_or(false)
    }

    #[inline]
    pub fn unsync_sender(&mut self) -> unsync::UnboundedSender<ContextProtocol<A>> {
        self.unsync_msgs.sender()
    }

    #[inline]
    pub fn unsync_address(&mut self) -> Address<A> {
        Address::new(self.unsync_msgs.sender())
    }

    pub fn sync_address(&mut self) -> SyncAddress<A> {
        if self.sync_msgs.is_none() {
            let (tx, rx) = sync::unbounded();
            self.sync_msgs = Some(rx);
            SyncAddress::new(tx)
        } else {
            if let Some(ref mut addr) = self.sync_msgs {
                return SyncAddress::new(addr.sender())
            }
            unreachable!();
        }
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context, stop: bool) -> ContextCellResult {
        let mut n_polls = NumPolls(0);
        loop {
            let mut not_ready = true;

            // unsync messages
            loop {
                match self.unsync_msgs.poll() {
                    Ok(Async::Ready(Some(msg))) => {
                        not_ready = false;
                        match msg {
                            ContextProtocol::Envelope(mut env) => {
                                env.handle(act, ctx)
                            }
                            ContextProtocol::Upgrade(tx) => {
                                let _ = tx.send(self.sync_address());
                            }
                        }
                    }
                    Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => break,
                }
                if ctx.waiting() {
                    return ContextCellResult::Ready;
                }
                debug_assert!(n_polls.inc() < MAX_SYNC_POLLS,
                              "Use Self::Context::notify() instead of direct use of address");
            }

            // sync messages
            if let Some(ref mut msgs) = self.sync_msgs {
                loop {
                    match msgs.poll() {
                        Ok(Async::Ready(Some(mut msg))) => {
                            not_ready = false;
                            msg.handle(act, ctx);
                        }
                        Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => break,
                    }
                    if ctx.waiting() {
                        return ContextCellResult::Ready;
                    }
                    debug_assert!(n_polls.inc() < MAX_SYNC_POLLS,
                                  "Use Self::Context::notify() instead of direct use of address");
                }
            }

            if not_ready {
                if stop {
                    self.close()
                }
                return ContextCellResult::Ready;
            }
        }
    }
}

pub struct ActorWaitCell<A> where A: Actor {
    act: PhantomData<A>,
    fut: Box<ActorFuture<Item=(), Error=(), Actor=A>>,
}

impl<A> ActorWaitCell<A> where A: Actor, A::Context: AsyncContext<A>,
{
    #[inline]
    pub fn new<F>(fut: F) -> ActorWaitCell<A>
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        ActorWaitCell {
            act: PhantomData,
            fut: Box::new(fut),
        }
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context, stop: bool) -> ContextCellResult {
        match self.fut.poll(act, ctx) {
            Ok(Async::NotReady) => {
                if !stop {
                    ContextCellResult::NotReady
                } else {
                    ContextCellResult::Completed
                }
            },
            Ok(Async::Ready(_)) => ContextCellResult::Ready,
            Err(_) => ContextCellResult::Completed,
        }
    }
}
