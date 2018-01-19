use std::mem;
use futures::{Async, Poll};

use fut::ActorFuture;
use queue::{sync, unsync};

use actor::{Actor, AsyncContext, ActorState, SpawnHandle};
use address::{Address, SyncAddress, Subscriber};
use context::AsyncContextApi;
use contextcells::{ContextCell, ContextCellResult, ContextProtocol,
                   ActorAddressCell, ActorItemsCell, ActorWaitCell};
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
        const WAITING =  0b0100_0000;
    }
}

/// A macro for processing `ContextCellResult`.
///
/// This macro bakes propagation of both errors and `NotReady` signals by
/// returning early.
macro_rules! cell_ready {
    ($slf:ident, $e:expr) => (match $e {
        ContextCellResult::Ready => (),
        ContextCellResult::NotReady => return Ok(Async::NotReady),
        ContextCellResult::Stop => $slf.stop(),
    })
}


/// Actor execution context impl
///
/// This is base Context implementation. It supports one extra cell
/// impl with type `ContextCell<A>` (i.e. `ActorFramedCell`)
pub struct ContextImpl<A, C=()>
    where A: Actor, A::Context: AsyncContext<A> + AsyncContextApi<A>
{
    act: Option<A>,
    flags: ContextFlags,
    wait: ActorWaitCell<A>,
    items: ActorItemsCell<A>,
    address: ActorAddressCell<A>,
    cell: Option<C>,
}

impl<A, C> ContextImpl<A, C>
    where A: Actor, A::Context: AsyncContext<A> + AsyncContextApi<A>, C: ContextCell<A>
{
    #[inline]
    pub fn new(act: Option<A>) -> ContextImpl<A, C> {
        ContextImpl {
            act: act,
            flags: ContextFlags::RUNNING,
            wait: ActorWaitCell::default(),
            items: ActorItemsCell::default(),
            address: ActorAddressCell::default(),
            cell: None,
        }
    }

    #[inline]
    pub fn with_cell(act: Option<A>, cell: C) -> ContextImpl<A, C> {
        ContextImpl {
            act: act,
            flags: ContextFlags::RUNNING,
            wait: ActorWaitCell::default(),
            items: ActorItemsCell::default(),
            address: ActorAddressCell::default(),
            cell: Some(cell),
        }
    }

    #[inline]
    pub fn with_receiver(act: Option<A>,
                         rx: sync::UnboundedReceiver<Envelope<A>>) -> ContextImpl<A, C> {
        ContextImpl {
            act: act,
            flags: ContextFlags::RUNNING,
            wait: ActorWaitCell::default(),
            items: ActorItemsCell::default(),
            address: ActorAddressCell::new(rx),
            cell: None,
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
    /// Mutable reference to cell
    pub fn cell(&mut self) -> &mut Option<C> {
        &mut self.cell
    }

    #[inline]
    /// Mark context as modified, this cause extra poll loop over all cells
    pub fn modify(&mut self) {
        self.flags.insert(ContextFlags::MODIFIED);
    }

    #[inline]
    /// Is context waiting for future completion
    pub fn wating(&self) -> bool {
        self.flags.contains(ContextFlags::WAITING)
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
        } else if self.flags.contains(ContextFlags::STOPPING | ContextFlags::PREPSTOP) {
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
        self.items.spawn(fut)
    }

    #[inline]
    /// Spawn new future to this context and wait future completion.
    ///
    /// During wait period actor does not receive any messages.
    pub fn wait<F>(&mut self, fut: F)
        where F: ActorFuture<Item=(), Error=(), Actor=A> + 'static
    {
        self.flags.insert(ContextFlags::MODIFIED);
        self.flags.insert(ContextFlags::WAITING);
        self.wait.add(fut)
    }

    #[inline]
    /// Cancel previously scheduled future.
    pub fn cancel_future(&mut self, handle: SpawnHandle) -> bool {
        self.flags.insert(ContextFlags::MODIFIED);
        self.items.cancel_future(handle)
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
    pub fn alive(&mut self) -> bool {
        if self.flags.contains(ContextFlags::STOPPED) {
            false
        } else {
            self.address.connected() || !self.items.is_empty() || !self.wait.is_empty() ||
                self.cell.as_ref().map(|c| c.alive()).unwrap_or(false)
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
        if self.act.is_none() {
            return Ok(Async::Ready(()))
        }
        let act: &mut A = unsafe {
            mem::transmute(self.act.as_mut().unwrap() as &mut A)
        };

        if !self.flags.contains(ContextFlags::STARTED) {
            Actor::started(act, ctx);
            self.flags.insert(ContextFlags::STARTED);
        }

        loop {
            self.flags.remove(ContextFlags::MODIFIED);
            let prepstop = self.flags.contains(ContextFlags::PREPSTOP);

            // check wait futures
            cell_ready!{ self, self.wait.poll(act, ctx, prepstop) };

            // check cell
            let stop = match self.cell {
                Some(ref mut cell) => match cell.poll(act, ctx, prepstop) {
                    ContextCellResult::Ready => false,
                    ContextCellResult::NotReady => return Ok(Async::NotReady),
                    ContextCellResult::Stop => true,
                },
                None => false,
            };
            if stop {
                self.stop();
            }
            self.flags.remove(ContextFlags::WAITING);

            // check for incoming messages
            cell_ready!{ self, self.address.poll(act, ctx, prepstop) };

            // process futures
            cell_ready!{ self, self.items.poll(act, ctx, prepstop) };

            // modified indicates that new IO item has been added during poll process
            if self.flags.contains(ContextFlags::MODIFIED) {
                continue
            }

            // check state
            if self.flags.contains(ContextFlags::RUNNING) {
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
