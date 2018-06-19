#![cfg_attr(
    feature = "cargo-clippy", allow(redundant_field_names, suspicious_arithmetic_impl)
)]

use futures::{Async, Poll};
use smallvec::SmallVec;

use actor::{Actor, ActorState, AsyncContext, Running, SpawnHandle, Supervised};
use address::{Addr, AddressReceiver};
use contextitems::ActorWaitItem;
use fut::ActorFuture;
use mailbox::Mailbox;

/// internal context state
bitflags! {
    struct ContextFlags: u8 {
        const STARTED =  0b0000_0001;
        const RUNNING =  0b0000_0010;
        const STOPPING = 0b0000_0100;
        const STOPPED =  0b0001_0000;
        const MODIFIED = 0b0010_0000;
    }
}

type Item<A> = (
    SpawnHandle,
    Box<ActorFuture<Item = (), Error = (), Actor = A>>,
);

/// Actor execution context impl
///
/// This is base Context implementation. Multiple cell's could be added.
pub struct ContextImpl<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    act: Option<A>,
    flags: ContextFlags,
    mailbox: Mailbox<A>,
    wait: SmallVec<[ActorWaitItem<A>; 2]>,
    items: SmallVec<[Item<A>; 3]>,
    handles: SmallVec<[SpawnHandle; 2]>,
}

impl<A> ContextImpl<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    #[inline]
    pub fn new(act: Option<A>) -> ContextImpl<A> {
        ContextImpl {
            act,
            wait: SmallVec::new(),
            items: SmallVec::new(),
            flags: ContextFlags::RUNNING,
            mailbox: Mailbox::default(),
            handles: SmallVec::from_slice(&[
                SpawnHandle::default(),
                SpawnHandle::default(),
            ]),
        }
    }

    #[inline]
    pub fn with_receiver(act: Option<A>, rx: AddressReceiver<A>) -> Self {
        ContextImpl {
            act,
            wait: SmallVec::new(),
            items: SmallVec::new(),
            flags: ContextFlags::RUNNING,
            mailbox: Mailbox::new(rx),
            handles: SmallVec::from_slice(&[
                SpawnHandle::default(),
                SpawnHandle::default(),
            ]),
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
        !self.wait.is_empty()
            || self
                .flags
                .intersects(ContextFlags::STOPPING | ContextFlags::STOPPED)
    }

    #[inline]
    /// Initiate stop process for actor execution
    ///
    /// Actor could prevent stopping by returning `false` from
    /// `Actor::stopping()` method.
    pub fn stop(&mut self) {
        if self.flags.contains(ContextFlags::RUNNING) {
            self.flags
                .remove(ContextFlags::RUNNING | ContextFlags::MODIFIED);
            self.flags.insert(ContextFlags::STOPPING);
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
        } else if self.flags.contains(ContextFlags::STOPPED) {
            ActorState::Stopped
        } else if self.flags.contains(ContextFlags::STOPPING) {
            ActorState::Stopping
        } else {
            ActorState::Started
        }
    }

    #[inline]
    /// Handle of the running future
    pub fn curr_handle(&self) -> SpawnHandle {
        self.handles[1]
    }

    #[inline]
    /// Spawn new future to this context.
    pub fn spawn<F>(&mut self, fut: F) -> SpawnHandle
    where
        F: ActorFuture<Item = (), Error = (), Actor = A> + 'static,
    {
        self.modify();
        let handle = self.handles[0].next();
        self.handles[0] = handle;
        let fut: Box<ActorFuture<Item = (), Error = (), Actor = A>> = Box::new(fut);
        self.items.push((handle, fut));
        handle
    }

    #[inline]
    /// Spawn new future to this context and wait future completion.
    ///
    /// During wait period actor does not receive any messages.
    pub fn wait<F>(&mut self, f: F)
    where
        F: ActorFuture<Item = (), Error = (), Actor = A> + 'static,
    {
        self.modify();
        self.wait.push(ActorWaitItem::new(f));
    }

    #[inline]
    /// Cancel previously scheduled future.
    pub fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.handles.push(handle);
        true
    }

    #[inline]
    pub fn capacity(&mut self) -> usize {
        self.mailbox.capacity()
    }

    #[inline]
    pub fn set_mailbox_capacity(&mut self, cap: usize) {
        self.modify();
        self.mailbox.set_capacity(cap);
    }

    #[inline]
    pub fn address(&self) -> Addr<A> {
        self.mailbox.address()
    }

    #[inline]
    pub fn alive(&self) -> bool {
        if self.flags.contains(ContextFlags::STOPPED) {
            false
        } else {
            !self.flags.contains(ContextFlags::STARTED)
                || self.mailbox.connected()
                || !self.items.is_empty()
                || !self.wait.is_empty()
        }
    }

    #[inline]
    fn stopping(&self) -> bool {
        self.flags
            .intersects(ContextFlags::STOPPING | ContextFlags::STOPPED)
    }

    /// Restart context. Cleanup all futures, except address queue.
    #[inline]
    pub fn restart(&mut self, ctx: &mut A::Context) -> bool
    where
        A: Supervised,
    {
        if self.act.is_none() || !self.mailbox.connected() {
            false
        } else {
            self.flags = ContextFlags::RUNNING;
            self.wait = SmallVec::new();
            self.items = SmallVec::new();
            self.handles[0] = SpawnHandle::default();
            self.actor().restarting(ctx);
            true
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
        let mut act = if let Some(act) = self.act.take() {
            act
        } else {
            return Ok(Async::Ready(()));
        };

        if !self.flags.contains(ContextFlags::STARTED) {
            self.flags.insert(ContextFlags::STARTED);
            Actor::started(&mut act, ctx);

            // check cancelled handles, just in case
            while self.handles.len() > 2 {
                let handle = self.handles.pop().unwrap();
                let mut idx = 0;
                while idx < self.items.len() {
                    if self.items[idx].0 == handle {
                        self.items.swap_remove(idx);
                    } else {
                        idx += 1;
                    }
                }
            }
        }

        'outer: loop {
            self.flags.remove(ContextFlags::MODIFIED);

            // check wait futures. order does matter
            // ctx.wait() always add to the back of the list
            // and we always have to check most recent future
            while !self.wait.is_empty() && !self.stopping() {
                let idx = self.wait.len() - 1;
                if let Some(item) = self.wait.last_mut() {
                    match item.poll(&mut act, ctx) {
                        Async::Ready(_) => (),
                        Async::NotReady => {
                            self.act = Some(act);
                            return Ok(Async::NotReady);
                        }
                    }
                }
                self.wait.remove(idx);
            }

            // process mailbox
            self.mailbox.poll(&mut act, ctx);
            if !self.wait.is_empty() && !self.stopping() {
                continue;
            }

            // process items
            let mut idx = 0;
            while idx < self.items.len() && !self.stopping() {
                self.handles[1] = self.items[idx].0;
                match self.items[idx].1.poll(&mut act, ctx) {
                    Ok(Async::NotReady) => {
                        // check cancelled handles
                        if self.handles.len() > 2 {
                            // this code is not very efficient, relaying on fact that
                            // cancellation should be rear also number of futures
                            // in actor context should be small
                            while self.handles.len() > 2 {
                                let handle = self.handles.pop().unwrap();
                                let mut idx = 0;
                                while idx < self.items.len() {
                                    if self.items[idx].0 == handle {
                                        self.items.swap_remove(idx);
                                    } else {
                                        idx += 1;
                                    }
                                }
                            }
                            continue 'outer;
                        }

                        // item scheduled wait future
                        if !self.wait.is_empty() && !self.stopping() {
                            // move current item to end of poll queue
                            // otherwise it is possible that same item generate wait
                            // future and prevents polling
                            // of other items
                            let next = self.items.len() - 1;
                            if idx != next {
                                self.items.swap(idx, next);
                            }
                            continue 'outer;
                        } else {
                            idx += 1;
                        }
                    }
                    Ok(Async::Ready(())) | Err(_) => {
                        self.items.swap_remove(idx);
                        // one of the items scheduled wait future
                        if !self.wait.is_empty() && !self.stopping() {
                            continue 'outer;
                        }
                    }
                }
            }
            self.handles[1] = SpawnHandle::default();

            // ContextFlags::MODIFIED indicates that new IO item has
            // been added during poll process
            if self.flags.contains(ContextFlags::MODIFIED)
                && !self.flags.contains(ContextFlags::STOPPING)
            {
                continue;
            }

            // check state
            if self.flags.contains(ContextFlags::RUNNING) {
                // possible stop condition
                if !self.alive() && Actor::stopping(&mut act, ctx) == Running::Stop {
                    self.flags = ContextFlags::STOPPED | ContextFlags::STARTED;
                    Actor::stopped(&mut act, ctx);
                    self.act = Some(act);
                    return Ok(Async::Ready(()));
                }
            } else if self.flags.contains(ContextFlags::STOPPING) {
                if Actor::stopping(&mut act, ctx) == Running::Stop {
                    self.flags = ContextFlags::STOPPED | ContextFlags::STARTED;
                    Actor::stopped(&mut act, ctx);
                    self.act = Some(act);
                    return Ok(Async::Ready(()));
                } else {
                    self.flags.remove(ContextFlags::STOPPING);
                    self.flags.insert(ContextFlags::RUNNING);
                    continue;
                }
            } else if self.flags.contains(ContextFlags::STOPPED) {
                Actor::stopped(&mut act, ctx);
                self.act = Some(act);
                return Ok(Async::Ready(()));
            }

            self.act = Some(act);
            return Ok(Async::NotReady);
        }
    }
}
