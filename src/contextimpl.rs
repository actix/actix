use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use bitflags::bitflags;
use futures_util::future::Future;
use smallvec::SmallVec;

use crate::actor::{
    Actor, ActorContext, ActorState, AsyncContext, Running, SpawnHandle, Supervised,
};
use crate::address::{Addr, AddressSenderProducer};
use crate::contextitems::ActorWaitItem;
use crate::fut::ActorFuture;
use crate::mailbox::Mailbox;

bitflags! {
    /// internal context state
    struct ContextFlags: u8 {
    const STARTED =  0b0000_0001;
    const RUNNING =  0b0000_0010;
    const STOPPING = 0b0000_0100;
    const STOPPED =  0b0001_0000;
    const MB_CAP_CHANGED = 0b0010_0000;
    }
}

type Item<A> = (
    SpawnHandle,
    Pin<Box<dyn ActorFuture<Output = (), Actor = A>>>,
);

pub trait AsyncContextParts<A>: ActorContext + AsyncContext<A>
where
    A: Actor<Context = Self>,
{
    fn parts(&mut self) -> &mut ContextParts<A>;
}

pub struct ContextParts<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    addr: AddressSenderProducer<A>,
    flags: ContextFlags,
    wait: SmallVec<[ActorWaitItem<A>; 2]>,
    items: SmallVec<[Item<A>; 3]>,
    handles: SmallVec<[SpawnHandle; 2]>,
}

impl<A> fmt::Debug for ContextParts<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("ContextParts")
            .field("flags", &self.flags)
            .finish()
    }
}

impl<A> ContextParts<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    #[inline]
    /// Create new ContextParts instance
    pub fn new(addr: AddressSenderProducer<A>) -> Self {
        ContextParts {
            addr,
            flags: ContextFlags::RUNNING,
            wait: SmallVec::new(),
            items: SmallVec::new(),
            handles: SmallVec::from_slice(&[
                SpawnHandle::default(),
                SpawnHandle::default(),
            ]),
        }
    }

    #[inline]
    /// Initiate stop process for actor execution
    ///
    /// Actor could prevent stopping by returning `false` from
    /// `Actor::stopping()` method.
    pub fn stop(&mut self) {
        if self.flags.contains(ContextFlags::RUNNING) {
            self.flags.remove(ContextFlags::RUNNING);
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
    /// Is context waiting for future completion
    pub fn waiting(&self) -> bool {
        !self.wait.is_empty()
            || self
                .flags
                .intersects(ContextFlags::STOPPING | ContextFlags::STOPPED)
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
        F: ActorFuture<Output = (), Actor = A> + 'static,
    {
        let handle = self.handles[0].next();
        self.handles[0] = handle;
        let fut: Box<dyn ActorFuture<Output = (), Actor = A>> = Box::new(fut);
        self.items.push((handle, Pin::from(fut)));
        handle
    }

    #[inline]
    /// Spawn new future to this context and wait future completion.
    ///
    /// During wait period actor does not receive any messages.
    pub fn wait<F>(&mut self, f: F)
    where
        F: ActorFuture<Output = (), Actor = A> + 'static,
    {
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
        self.addr.capacity()
    }

    #[inline]
    pub fn set_mailbox_capacity(&mut self, cap: usize) {
        self.flags.insert(ContextFlags::MB_CAP_CHANGED);
        self.addr.set_capacity(cap);
    }

    #[inline]
    pub fn address(&self) -> Addr<A> {
        Addr::new(self.addr.sender())
    }

    /// Restart context. Cleanup all futures, except address queue.
    #[inline]
    pub(crate) fn restart(&mut self) {
        self.flags = ContextFlags::RUNNING;
        self.wait = SmallVec::new();
        self.items = SmallVec::new();
        self.handles[0] = SpawnHandle::default();
    }

    #[inline]
    pub fn started(&mut self) -> bool {
        self.flags.contains(ContextFlags::STARTED)
    }

    /// Are any senders connected
    #[inline]
    pub fn connected(&self) -> bool {
        self.addr.connected()
    }
}

pub struct ContextFut<A, C>
where
    C: AsyncContextParts<A> + Unpin,
    A: Actor<Context = C>,
{
    ctx: C,
    act: A,
    mailbox: Mailbox<A>,
    wait: SmallVec<[ActorWaitItem<A>; 2]>,
    items: SmallVec<[Item<A>; 3]>,
}

impl<A, C> fmt::Debug for ContextFut<A, C>
where
    C: AsyncContextParts<A> + Unpin,
    A: Actor<Context = C>,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "ContextFut {{ /* omitted */ }}")
    }
}

impl<A, C> Drop for ContextFut<A, C>
where
    C: AsyncContextParts<A> + Unpin,
    A: Actor<Context = C>,
{
    fn drop(&mut self) {
        if self.alive() && actix_rt::Arbiter::is_running() {
            self.ctx.parts().stop();
            let waker = futures_util::task::noop_waker();
            let mut cx = futures_util::task::Context::from_waker(&waker);
            let _ = Pin::new(self).poll(&mut cx);
        }
    }
}

impl<A, C> ContextFut<A, C>
where
    C: AsyncContextParts<A> + Unpin,
    A: Actor<Context = C>,
{
    pub fn new(ctx: C, act: A, mailbox: Mailbox<A>) -> Self {
        ContextFut {
            ctx,
            act,
            mailbox,
            wait: SmallVec::new(),
            items: SmallVec::new(),
        }
    }

    #[inline]
    pub fn ctx(&mut self) -> &mut C {
        &mut self.ctx
    }

    #[inline]
    pub fn address(&self) -> Addr<A> {
        self.mailbox.address()
    }

    #[inline]
    fn stopping(&mut self) -> bool {
        self.ctx
            .parts()
            .flags
            .intersects(ContextFlags::STOPPING | ContextFlags::STOPPED)
    }

    #[inline]
    pub fn alive(&mut self) -> bool {
        if self.ctx.parts().flags.contains(ContextFlags::STOPPED) {
            false
        } else {
            !self.ctx.parts().flags.contains(ContextFlags::STARTED)
                || self.mailbox.connected()
                || !self.items.is_empty()
                || !self.wait.is_empty()
        }
    }

    /// Restart context. Cleanup all futures, except address queue.
    #[inline]
    pub(crate) fn restart(&mut self) -> bool
    where
        A: Supervised,
    {
        if self.mailbox.connected() {
            self.wait = SmallVec::new();
            self.items = SmallVec::new();
            self.ctx.parts().restart();
            self.act.restarting(&mut self.ctx);
            true
        } else {
            false
        }
    }

    fn merge(&mut self) -> bool {
        let mut modified = false;

        let parts = self.ctx.parts();
        if !parts.wait.is_empty() {
            modified = true;
            self.wait.extend(parts.wait.drain(0..));
        }
        if !parts.items.is_empty() {
            modified = true;
            self.items.extend(parts.items.drain(0..));
        }
        //
        if parts.flags.contains(ContextFlags::MB_CAP_CHANGED) {
            modified = true;
            parts.flags.remove(ContextFlags::MB_CAP_CHANGED);
        }
        if parts.handles.len() > 2 {
            modified = true;
        }

        modified
    }

    fn clean_canceled_handle(&mut self) {
        while self.ctx.parts().handles.len() > 2 {
            let handle = self.ctx.parts().handles.pop().unwrap();
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
}

#[doc(hidden)]
impl<A, C> Future for ContextFut<A, C>
where
    C: AsyncContextParts<A> + Unpin,
    A: Actor<Context = C>,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        if !this.ctx.parts().flags.contains(ContextFlags::STARTED) {
            this.ctx.parts().flags.insert(ContextFlags::STARTED);
            Actor::started(&mut this.act, &mut this.ctx);

            // check cancelled handles, just in case
            if this.merge() {
                this.clean_canceled_handle();
            }
        }

        'outer: loop {
            // check wait futures. order does matter
            // ctx.wait() always add to the back of the list
            // and we always have to check most recent future
            while !this.wait.is_empty() && !this.stopping() {
                let idx = this.wait.len() - 1;
                if let Some(item) = this.wait.last_mut() {
                    match Pin::new(item).poll(&mut this.act, &mut this.ctx, cx) {
                        Poll::Ready(_) => (),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                this.wait.remove(idx);
                this.merge();
            }

            // process mailbox
            this.mailbox.poll(&mut this.act, &mut this.ctx, cx);
            if !this.wait.is_empty() && !this.stopping() {
                continue;
            }

            // process items
            let mut idx = 0;
            while idx < this.items.len() && !this.stopping() {
                this.ctx.parts().handles[1] = this.items[idx].0;
                match Pin::new(&mut this.items[idx].1).poll(
                    &mut this.act,
                    &mut this.ctx,
                    cx,
                ) {
                    Poll::Pending => {
                        // check cancelled handles
                        if this.ctx.parts().handles.len() > 2 {
                            // this code is not very efficient, relaying on fact that
                            // cancellation should be rear also number of futures
                            // in actor context should be small
                            this.clean_canceled_handle();

                            continue 'outer;
                        }

                        // item scheduled wait future
                        if !this.wait.is_empty() && !this.stopping() {
                            // move current item to end of poll queue
                            // otherwise it is possible that same item generate wait
                            // future and prevents polling
                            // of other items
                            let next = this.items.len() - 1;
                            if idx != next {
                                this.items.swap(idx, next);
                            }
                            continue 'outer;
                        } else {
                            idx += 1;
                        }
                    }
                    Poll::Ready(()) => {
                        this.items.swap_remove(idx);
                        // one of the items scheduled wait future
                        if !this.wait.is_empty() && !this.stopping() {
                            continue 'outer;
                        }
                    }
                }
            }
            this.ctx.parts().handles[1] = SpawnHandle::default();

            // merge returns true if context contains new items or handles to be cancelled
            if this.merge() && !this.ctx.parts().flags.contains(ContextFlags::STOPPING) {
                // if we have no item to process, cancelled handles wouldn't be
                // reaped in the above loop. this means this.merge() will never
                // be false and the poll() never ends. so, discard the handles
                // as we're sure there are no more items to be cancelled.
                if this.items.is_empty() {
                    this.ctx.parts().handles.truncate(2);
                }
                continue;
            }

            // check state
            if this.ctx.parts().flags.contains(ContextFlags::RUNNING) {
                // possible stop condition
                if !this.alive()
                    && Actor::stopping(&mut this.act, &mut this.ctx) == Running::Stop
                {
                    this.ctx.parts().flags =
                        ContextFlags::STOPPED | ContextFlags::STARTED;
                    Actor::stopped(&mut this.act, &mut this.ctx);
                    return Poll::Ready(());
                }
            } else if this.ctx.parts().flags.contains(ContextFlags::STOPPING) {
                if Actor::stopping(&mut this.act, &mut this.ctx) == Running::Stop {
                    this.ctx.parts().flags =
                        ContextFlags::STOPPED | ContextFlags::STARTED;
                    Actor::stopped(&mut this.act, &mut this.ctx);
                    return Poll::Ready(());
                } else {
                    this.ctx.parts().flags.remove(ContextFlags::STOPPING);
                    this.ctx.parts().flags.insert(ContextFlags::RUNNING);
                    continue;
                }
            } else if this.ctx.parts().flags.contains(ContextFlags::STOPPED) {
                Actor::stopped(&mut this.act, &mut this.ctx);
                return Poll::Ready(());
            }

            return Poll::Pending;
        }
    }
}
