use std::mem;

use futures::{Async, Poll};
use smallvec::SmallVec;

use fut::ActorFuture;
use queue::{sync, unsync};

use actor::{Actor, AsyncContext, ActorState, SpawnHandle};
use address::{Address, SyncAddress, Subscriber};
use context::{AsyncContextApi, ContextProtocol};
use contextitems::ActorWaitItem;
use contextaddress::ContextAddress;
use handler::{Handler, ResponseType};
use envelope::Envelope;


bitflags! {
    struct ContextFlags: u8 {
        const STARTED =  0b0000_0001;
        const RUNNING =  0b0000_0010;
        const STOPPING = 0b0000_0100;
        const PREPSTOP = 0b0000_1000;
        const STOPPED =  0b0001_0000;
        const MODIFIED = 0b0010_0000;
    }
}

type Item<A> = (SpawnHandle, Box<ActorFuture<Item=(), Error=(), Actor=A>>);

/// Actor execution context impl
///
/// This is base Context implementation. Multiple cell's could be added.
pub struct ContextImpl<A> where A: Actor, A::Context: AsyncContext<A> {
    act: Option<A>,
    flags: ContextFlags,
    address: ContextAddress<A>,
    wait: SmallVec<[ActorWaitItem<A>; 2]>,
    items: SmallVec<[Item<A>; 3]>,
    handle: SpawnHandle,
}

impl<A> ContextImpl<A> where A: Actor, A::Context: AsyncContext<A> + AsyncContextApi<A>
{
    #[inline]
    pub fn new(act: Option<A>) -> ContextImpl<A> {
        ContextImpl {
            act: act,
            wait: SmallVec::new(),
            items: SmallVec::new(),
            flags: ContextFlags::RUNNING,
            handle: SpawnHandle::default(),
            address: ContextAddress::default(),
        }
    }

    #[inline]
    pub fn with_receiver(act: Option<A>, rx: sync::UnboundedReceiver<Envelope<A>>) -> Self {
        ContextImpl {
            act: act,
            wait: SmallVec::new(),
            items: SmallVec::new(),
            flags: ContextFlags::RUNNING,
            handle: SpawnHandle::default(),
            address: ContextAddress::new(rx),
        }
    }

    #[inline]
    /// Mutable reference to an actor.
    ///
    /// It panics if actor is not set
    pub fn actor(&mut self) -> &mut A {
        self.act.as_mut().unwrap()
    }

    #[inline]
    /// Mark context as modified, this cause extra poll loop over all items
    pub fn modify(&mut self) {
        self.flags.insert(ContextFlags::MODIFIED);
    }

    #[inline]
    /// Is context waiting for future completion
    pub fn waiting(&self) -> bool {
        !self.wait.is_empty() ||
            self.flags.intersects(ContextFlags::STOPPING | ContextFlags::PREPSTOP)
    }

    #[inline]
    /// Initiate stop process for actor execution
    ///
    /// Actor could prevent stopping by returning `false` from `Actor::stopping()` method.
    pub fn stop(&mut self) {
        if self.flags.contains(ContextFlags::RUNNING) {
            self.flags.remove(ContextFlags::RUNNING);
            self.flags.insert(ContextFlags::STOPPING | ContextFlags::MODIFIED);
        }
    }

    #[inline]
    /// Terminate actor execution
    pub fn terminate(&mut self) {
        self.flags = ContextFlags::STOPPED;
    }

    #[inline]
    /// Actor execution state
    pub fn state(&self) -> ActorState {
        if self.flags.contains(ContextFlags::RUNNING) {
            ActorState::Running
        } else if self.flags.intersects(ContextFlags::STOPPING | ContextFlags::PREPSTOP) {
            ActorState::Stopping
        } else if self.flags.contains(ContextFlags::STOPPED) {
            ActorState::Stopped
        } else {
            ActorState::Started
        }
    }

    #[inline]
    /// Spawn new future to this context.
    pub fn spawn<F>(&mut self, fut: F) -> SpawnHandle
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.flags.insert(ContextFlags::MODIFIED);
        self.handle = self.handle.next();
        let fut: Box<ActorFuture<Item=(), Error=(), Actor=A>> = Box::new(fut);
        self.items.push((self.handle, fut));
        self.handle
    }

    #[inline]
    /// Spawn new future to this context and wait future completion.
    ///
    /// During wait period actor does not receive any messages.
    pub fn wait<F>(&mut self, f: F) where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static {
        self.wait.push(ActorWaitItem::new(f));
        self.flags.insert(ContextFlags::MODIFIED);
    }

    #[inline]
    /// Cancel previously scheduled future.
    pub fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        for idx in 0..self.items.len() {
            if self.items[idx].0 == handle {
                self.flags.insert(ContextFlags::MODIFIED);
                self.items.swap_remove(idx);
                return true
            }
        }
        false
    }

    #[inline]
    pub fn unsync_sender(&mut self) -> unsync::UnboundedSender<ContextProtocol<A>> {
        self.modify();
        self.address.unsync_sender()
    }

    #[inline]
    pub fn unsync_address(&mut self) -> Address<A> {
        self.modify();
        self.address.unsync_address()
    }

    #[inline]
    pub fn sync_address(&mut self) -> SyncAddress<A> {
        self.modify();
        self.address.sync_address()
    }

    #[inline]
    pub fn subscriber<M>(&mut self) -> Box<Subscriber<M>>
        where A: Handler<M>,
              M: ResponseType + 'static {
        self.modify();
        Box::new(self.address.unsync_address())
    }

    #[inline]
    pub fn sync_subscriber<M>(&mut self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              M: ResponseType + Send + 'static, M::Item: Send, M::Error: Send {
        self.modify();
        Box::new(self.address.sync_address())
    }

    #[inline]
    pub fn alive(&self) -> bool {
        if self.flags.contains(ContextFlags::STOPPED) {
            false
        } else {
            self.address.connected() || !self.items.is_empty()
        }
    }

    #[inline]
    pub fn set_actor(&mut self, act: A) {
        self.act = Some(act);
        self.modify();
    }

    #[inline]
    pub fn into_inner(self) -> Option<A> {
        self.act
    }

    #[inline]
    pub fn started(&mut self) -> bool {
        self.flags.contains(ContextFlags::STARTED)
    }

    pub fn poll(&mut self, ctx: &mut A::Context) -> Poll<(), ()> {
        let act: &mut A = if let Some(ref mut act) = self.act {
            unsafe { mem::transmute(act) }
        } else {
            return Ok(Async::Ready(()))
        };

        if !self.flags.contains(ContextFlags::STARTED) {
            Actor::started(act, ctx);
            self.flags.insert(ContextFlags::STARTED);
        }

        'outer: loop {
            self.flags.remove(ContextFlags::MODIFIED);
            let prepstop = self.flags.contains(ContextFlags::PREPSTOP);

            // check wait futures
            while !self.wait.is_empty() {
                match self.wait[0].poll(act, ctx, prepstop) {
                    Async::Ready(_) => { self.wait.swap_remove(0); },
                    Async::NotReady => return Ok(Async::NotReady),
                }
            }

            // process address
            self.address.poll(act, ctx, prepstop);
            if !self.wait.is_empty() {
                continue
            }

            // process items
            let mut idx = 0;
            while idx < self.items.len() {
                match self.items[idx].1.poll(act, ctx) {
                    Ok(Async::NotReady) => {
                        // item scheduled wait future
                        if !self.wait.is_empty() {
                            // move current item to end of poll queue
                            // otherwise it is possible that same item generate wait future
                            // and prevents polling of other items
                            let next = self.items.len()-1;
                            if idx != next {
                                self.items.swap(idx, next);
                            }
                            continue 'outer
                        } else {
                            idx += 1;
                        }
                    },
                    Ok(Async::Ready(())) | Err(_) => {
                        self.items.swap_remove(idx);
                        // one of the items scheduled wait future
                        if !self.wait.is_empty() {
                            continue 'outer
                        }
                    },
                }
            }

            // ContextFlags::MODIFIED indicates that new IO item has
            // been added during poll process
            if self.flags.contains(ContextFlags::MODIFIED) {
                continue
            }

            // check state
            if self.flags.contains(ContextFlags::RUNNING) {
                // possible stop condition
                if !self.alive() && Actor::stopping(act, ctx) {
                    self.flags.remove(ContextFlags::RUNNING);
                    self.flags.insert(ContextFlags::PREPSTOP);
                    continue
                }
            } else if self.flags.contains(ContextFlags::STOPPING) {
                self.flags.remove(ContextFlags::STOPPING);
                if Actor::stopping(act, ctx) {
                    self.flags.insert(ContextFlags::PREPSTOP);
                } else {
                    self.flags.insert(ContextFlags::RUNNING);
                }
                continue
            } else if self.flags.contains(ContextFlags::PREPSTOP) {
                self.flags = ContextFlags::STOPPED;
                Actor::stopped(act, ctx);
                return Ok(Async::Ready(()))
            } else if self.flags.contains(ContextFlags::STOPPED) {
                Actor::stopped(act, ctx);
                return Ok(Async::Ready(()))
            }

            return Ok(Async::NotReady)
        }
    }
}
