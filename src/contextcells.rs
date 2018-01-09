use std::marker::PhantomData;
use std::collections::VecDeque;

use futures::{Async, Stream};
use futures::unsync::oneshot::Sender;
use smallvec::SmallVec;

use fut::ActorFuture;
use queue::{sync, unsync};

use actor::{Actor, AsyncContext, SpawnHandle};
use address::{Address, SyncAddress};
use envelope::Envelope;
use constants::MAX_SYNC_POLLS;


#[derive(Debug, PartialEq)]
pub enum ContextCellResult {
    NotReady,
    Ready,
    Stop,
}

/// context protocol
pub enum ContextProtocol<A: Actor> {
    /// message envelope
    Envelope(Envelope<A>),
    /// Request sync address
    Upgrade(Sender<SyncAddress<A>>),
}

pub trait ContextCell<A> where Self: 'static, A: Actor {
    fn alive(&self) -> bool;

    fn poll(&mut self, act: &mut A, ctx: &mut A::Context, stop: bool) -> ContextCellResult;
}

impl<A: Actor> ContextCell<A> for () {
    fn alive(&self) -> bool {
        false
    }
    fn poll(&mut self, _: &mut A, _: &mut A::Context, _: bool) -> ContextCellResult {
        ContextCellResult::Ready
    }
}

pub struct ActorAddressCell<A> where A: Actor, A::Context: AsyncContext<A> {
    sync_alive: bool,
    sync_msgs: Option<sync::UnboundedReceiver<Envelope<A>>>,
    unsync_msgs: unsync::UnboundedReceiver<ContextProtocol<A>>,
}

impl<A> Default for ActorAddressCell<A> where A: Actor, A::Context: AsyncContext<A> {

    #[inline]
    fn default() -> Self {
        ActorAddressCell {
            sync_alive: false,
            sync_msgs: None,
            unsync_msgs: unsync::unbounded(),
        }
    }
}

impl<A> ActorAddressCell<A> where A: Actor, A::Context: AsyncContext<A>
{
    #[inline]
    pub fn new(rx: sync::UnboundedReceiver<Envelope<A>>) -> Self {
        ActorAddressCell {
            sync_alive: true,
            sync_msgs: Some(rx),
            unsync_msgs: unsync::unbounded(),
        }
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
        self.unsync_msgs.connected() || self.sync_alive
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
            self.sync_alive = true;
            SyncAddress::new(tx)
        } else {
            if let Some(ref mut addr) = self.sync_msgs {
                return SyncAddress::new(addr.sender())
            }
            unreachable!();
        }
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context, stop: bool) -> ContextCellResult {
        let mut n_polls: u32 = 0;
        loop {
            let mut not_ready = true;
            n_polls += 1;

            // unsync messages
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
                Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => (),
            }

            // sync messages
            if self.sync_alive {
                if let Some(ref mut msgs) = self.sync_msgs {
                    match msgs.poll() {
                        Ok(Async::Ready(Some(mut msg))) => {
                            not_ready = false;
                            msg.handle(act, ctx);
                        }
                        Ok(Async::Ready(None)) | Err(_) => {
                            self.sync_alive = false;
                        },
                        Ok(Async::NotReady) => (),
                    }
                }
            }

            if not_ready || n_polls == MAX_SYNC_POLLS {
                if stop {
                    self.close()
                }
                return ContextCellResult::Ready;
            }
        }
    }
}

type Item<A> = (SpawnHandle, Option<Box<ActorFuture<Item=(), Error=(), Actor=A>>>);

pub struct ActorItemsCell<A> where A: Actor, A::Context: AsyncContext<A> {
    index: SpawnHandle,
    items: SmallVec<[Item<A>; 2]>,
}

impl<A> Default for ActorItemsCell<A> where A: Actor, A::Context: AsyncContext<A> {

    #[inline]
    fn default() -> Self {
        ActorItemsCell {
            index: SpawnHandle::default(),
            items: SmallVec::new(),
        }
    }
}

impl<A> ActorItemsCell<A> where A: Actor, A::Context: AsyncContext<A>
{
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[inline]
    pub fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        let fut: Box<ActorFuture<Item=(), Error=(), Actor=A>> = Box::new(fut);
        self.index = self.index.next();
        self.items.push((self.index, Some(fut)));
        self.index
    }

    #[inline]
    pub fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        for item in &mut self.items {
            if item.0 == handle {
                item.1.take();
                return true
            }
        }
        false
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context, stop: bool) -> ContextCellResult {
        loop {
            let mut idx = 0;
            let mut len = self.items.len();
            let mut not_ready = true;

            while idx < len {
                let drop = if let Some(ref mut item) = self.items[idx].1 {
                    match item.poll(act, ctx) {
                        Ok(val) => match val {
                            Async::Ready(_) => {
                                not_ready = false;
                                true
                            }
                            Async::NotReady => false,
                        },
                        Err(_) => true,
                    }
                } else { true };

                // number of items could be different, context can add more items
                len = self.items.len();

                // item finishes, we need to remove it,
                // replace current item with last item
                if drop {
                    len -= 1;
                    if idx >= len {
                        self.items.pop();
                        not_ready = true;
                        break
                    } else {
                        self.items[idx] = self.items.pop().unwrap();
                    }
                } else {
                    idx += 1;
                }
            }

            // are we done
            if not_ready {
                if stop {
                    self.items.clear();
                }
                return ContextCellResult::Ready
            }
        }
    }
}

pub struct ActorWaitCell<A>
    where A: Actor, A::Context: AsyncContext<A>,
{
    act: PhantomData<A>,
    fut: VecDeque<Box<ActorFuture<Item=(), Error=(), Actor=A>>>,
}

impl<A> Default for ActorWaitCell<A>
    where A: Actor, A::Context: AsyncContext<A>
{
    #[inline]
    fn default() -> ActorWaitCell<A> {
        ActorWaitCell {
            act: PhantomData,
            fut: VecDeque::new() }
    }
}

impl<A> ActorWaitCell<A>
    where A: Actor, A::Context: AsyncContext<A>,
{
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.fut.is_empty()
    }

    #[inline]
    pub fn add<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.fut.push_back(Box::new(fut));
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context, stop: bool) -> ContextCellResult {
        loop {
            if let Some(fut) = self.fut.front_mut() {
                match fut.poll(act, ctx) {
                    Ok(Async::NotReady) => {
                        if !stop {
                            return ContextCellResult::NotReady
                        }
                    },
                    Ok(Async::Ready(_)) | Err(_) => (),
                }
            }
            if stop {
                self.fut.clear();
            }
            if self.fut.is_empty() {
                return ContextCellResult::Ready
            } else {
                self.fut.pop_front();
            }
        }
    }
}
