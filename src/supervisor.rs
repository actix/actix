use std;
use futures::{Future, Async, Poll, Stream};

use actor::{Actor, SupervisedActor};
use arbiter::{Arbiter, Execute};
use address::{Address, SyncAddress, Proxy};
use context::{Context, ContextProtocol};
use queue::{sync, unsync};

/// Actor supervisor
///
/// `Supervisor` can not garantee that actor successfully process incoming message.
/// If actor fails during message processing, this message can not be recovered. But sender
/// would receive `Err(Cancelled)` error in this situation.
///
/// ## Example
///
/// ```rust
/// extern crate actix;
///
/// use actix::prelude::*;
///
/// // message
/// struct Die;
///
/// struct MyActor;
///
/// impl Actor for MyActor {}
///
/// // To use actor with supervisor actor has to implement `SupervisedActor` trait
/// impl SupervisedActor for MyActor {
///     fn restarting(&mut self, ctx: &mut Context<MyActor>) {
///         println!("restarting");
///     }
/// }
///
/// impl MessageResponse<Die> for MyActor {
///     type Item = ();
///     type Error = ();
/// }
///
/// impl MessageHandler<Die> for MyActor {
///
///     fn handle(&mut self, _: Die, ctx: &mut Context<MyActor>) -> MessageFuture<Self, Die> {
///         ctx.stop();
///         Arbiter::system().send(actix::SystemExit(0));
///         ().to_result()
///     }
/// }
///
/// fn main() {
///     let sys = System::new("test".to_owned());
///
///     let (addr, _) = Supervisor::start(false, |_| MyActor);
///
///     addr.send(Die);
///     sys.run();
/// }
/// ```
pub struct Supervisor<A: SupervisedActor> {
    cell: Option<ActorCell<A>>,
    factory: Option<Box<FnFactory<A>>>,
    msgs: unsync::UnboundedReceiver<ContextProtocol<A>>,
    sync_msgs: sync::UnboundedReceiver<Proxy<A>>,
    msg: Option<ContextProtocol<A>>,
    sync_msg: Option<Proxy<A>>,
}

struct ActorCell<A: SupervisedActor> {
    ctx: Context<A>,
    addr: unsync::UnboundedSender<ContextProtocol<A>>,
}

impl<A> Supervisor<A> where A: SupervisedActor
{
    /// Start new supervised actor. Depends on `lazy` argument actor could be started
    /// immidietly or on first incoming message.
    pub fn start<F>(lazy: bool, f: F) -> (Address<A>, SyncAddress<A>)
        where F: FnOnce(&mut Context<A>) -> A + 'static
    {
        // create actor
        let (cell, factory) = if !lazy {
            let mut ctx = Context::new(unsafe{std::mem::uninitialized()});
            let addr = ctx.address_cell().unsync_sender();
            let act = f(&mut ctx);
            let old = ctx.replace_actor(act);
            std::mem::forget(old);
            (Some(ActorCell{ctx: ctx, addr: addr}), None)
        } else {
            let f: Box<FnFactory<A>> = Box::new(f);
            (None, Some(f))
        };

        // create supervisor
        let rx = unsync::unbounded();
        let (stx, srx) = sync::unbounded();
        let mut supervisor = Supervisor {
            cell: cell,
            factory: factory,
            msgs: rx,
            sync_msgs: srx,
            msg: None,
            sync_msg: None,
        };
        let addr = Address::new(supervisor.msgs.sender());
        let saddr = SyncAddress::new(stx);
        Arbiter::handle().spawn(supervisor);

        (addr, saddr)
    }

    /// Start new supervised actor in arbiter's thread. Depends on `lazy` argument
    /// actor could be started immidietly or on first incoming message.
    pub fn start_in<F>(addr: SyncAddress<Arbiter>, lazy: bool, f: F) -> Option<SyncAddress<A>>
        where F: FnOnce(&mut Context<A>) -> A + Send + 'static
    {
        if addr.is_closed() {
            None
        } else {
            let (tx, rx) = sync::unbounded();

            addr.send(Execute::new(move || -> Result<(), ()> {
                // create actor
                let (cell, factory) = if lazy {
                    let mut ctx = Context::new(unsafe{std::mem::uninitialized()});
                    let addr = ctx.address_cell().unsync_sender();
                    let act = f(&mut ctx);
                    let old = ctx.replace_actor(act);
                    std::mem::forget(old);
                    (Some(ActorCell{ctx: ctx, addr: addr}), None)
                } else {
                    let f: Box<FnFactory<A>> = Box::new(f);
                    (None, Some(f))
                };

                let lrx = unsync::unbounded();
                let supervisor = Supervisor {
                    cell: cell,
                    factory: factory,
                    msgs: lrx,
                    sync_msgs: rx,
                    msg: None,
                    sync_msg: None,
                };
                Arbiter::handle().spawn(supervisor);
                Ok(())
            }));

            if addr.is_closed() {
                None
            } else {
                Some(SyncAddress::new(tx))
            }
        }
    }

    fn get_cell(&mut self) -> &mut ActorCell<A> {
        if self.cell.is_none() {
            let f = self.factory.take().expect("Should be available");
            let mut ctx = Context::new(unsafe{std::mem::uninitialized()});

            let addr = ctx.address_cell().unsync_sender();
            let act = f.call(&mut ctx);
            let old = ctx.replace_actor(act);
            std::mem::forget(old);

            self.cell = Some(ActorCell {ctx: ctx, addr: addr});
        }
        self.cell.as_mut().unwrap()
    }

    fn restart(&mut self) {
        let cell = self.cell.take().unwrap();
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()});

        let addr = ctx.address_cell().unsync_sender();
        let old = ctx.replace_actor(cell.ctx.into_inner());
        std::mem::forget(old);
        ctx.restarting();

        self.cell = Some(ActorCell {ctx: ctx, addr: addr});
    }
}

#[doc(hidden)]
impl<A> Future for Supervisor<A> where A: SupervisedActor
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            // poll supervised actor
            if self.cell.is_some() {
                match self.get_cell().ctx.poll() {
                    Ok(Async::NotReady) => (),
                    Ok(Async::Ready(_)) | Err(_) => {
                        self.restart();
                    }
                }
            }

            let mut not_ready = true;

            // check messages
            if let Some(msg) = self.msg.take() {
                match msg {
                    ContextProtocol::SyncAddress(_) => (),
                    // if Actor message queue is dead, store in temp
                    msg => {
                        if let Err(msg) = self.get_cell().addr.unbounded_send(msg) {
                            self.msg = Some(msg.into_inner());
                        }
                    }
                }
            }
            if self.msg.is_none() {
                match self.msgs.poll() {
                    Ok(Async::Ready(Some(msg))) => {
                        not_ready = false;
                        match msg {
                            ContextProtocol::SyncAddress(tx) => {
                                let _ = tx.send(SyncAddress::new(self.sync_msgs.sender()));
                            }
                            // if Actor message queue is dead, store in temp
                            msg => {
                                if let Err(msg) = self.get_cell().addr.unbounded_send(msg) {
                                    self.msg = Some(msg.into_inner());
                                }
                            },
                        }
                    }
                    Ok(Async::NotReady) | Ok(Async::Ready(None)) | Err(_) => (),
                }
            }

            // check remote messages. we still use local queue for remote message,
            // because actor runs in same context as supervisor
            if let Some(msg) = self.sync_msg.take() {
                // if Actor message queue is dead, store in temp
                if let Err(msg) = self.get_cell().addr.unbounded_send(
                    ContextProtocol::Envelope(msg))
                {
                    if let ContextProtocol::Envelope(msg) = msg.into_inner() {
                        self.sync_msg = Some(msg);
                    }
                }
            }
            if self.sync_msg.is_none() {
                match self.sync_msgs.poll() {
                    Ok(Async::Ready(Some(msg))) => {
                        not_ready = false;
                        if let Err(msg) = self.get_cell().addr.unbounded_send(
                            ContextProtocol::Envelope(msg))
                        {
                            if let ContextProtocol::Envelope(msg) = msg.into_inner() {
                                self.sync_msg = Some(msg);
                            }
                        }
                    },
                    Ok(Async::NotReady) | Ok(Async::Ready(None)) | Err(_) => (),
                }
            }

            // are we done
            if not_ready {
                return Ok(Async::NotReady)
            }

            // poll supervised actor
            match self.get_cell().ctx.poll() {
                Ok(Async::NotReady) => (),
                Ok(Async::Ready(_)) | Err(_) => {
                    self.restart();
                }
            }
        }
    }
}

trait FnFactory<A: Actor>: 'static {
    fn call(self: Box<Self>, &mut Context<A>) -> A;
}

impl<A: Actor, F: FnOnce(&mut Context<A>) -> A + 'static> FnFactory<A> for F {
    #[cfg_attr(feature="cargo-clippy", allow(boxed_local))]
    fn call(self: Box<Self>, ctx: &mut Context<A>) -> A {
        (*self)(ctx)
    }
}
