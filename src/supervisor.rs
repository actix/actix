use futures::{Future, Async, Poll, Stream};

use actor::{Actor, Supervised, AsyncContext};
use arbiter::Arbiter;
use address::{Address, SyncAddress};
use context::{Context, ContextProtocol, AsyncContextApi};
use envelope::Envelope;
use msgs::Execute;
use queue::{sync, unsync};

/// Actor supervisor
///
/// Supervisor manages incoming message for actor. In case of actor failure, supervisor
/// creates new execution context and restarts actor lifecycle. Actor can be
/// constructed lazily.
///
/// Supervisor has same livecycle as actor. In situation when all addresses to supervisor
/// get dropped and actor does not execution anything supervisor terminates.
///
/// `Supervisor` can not guarantee that actor successfully process incoming message.
/// If actor fails during message processing, this message can not be recovered. Sender
/// would receive `Err(Cancelled)` error in this situation.
///
/// ## Example
///
/// ```rust
/// # #[macro_use] extern crate actix;
/// use actix::prelude::*;
///
/// #[derive(Message)]
/// struct Die;
///
/// struct MyActor;
///
/// impl Actor for MyActor {
///     type Context = Context<Self>;
/// }
///
/// // To use actor with supervisor actor has to implement `Supervised` trait
/// impl actix::Supervised for MyActor {
///     fn restarting(&mut self, ctx: &mut Context<MyActor>) {
///         println!("restarting");
///     }
/// }
///
/// impl Handler<Die> for MyActor {
///     type Result = ();
///
///     fn handle(&mut self, _: Die, ctx: &mut Context<MyActor>) {
///         ctx.stop();
/// #       Arbiter::system().send(actix::msgs::SystemExit(0));
///     }
/// }
///
/// fn main() {
///     let sys = System::new("test");
///
///     let (addr, _) = actix::Supervisor::start(false, |_| MyActor);
///
///     addr.send(Die);
///     sys.run();
/// }
/// ```
pub struct Supervisor<A: Supervised> where A: Actor<Context=Context<A>> {
    cell: Option<ActorCell<A>>,
    factory: Option<Box<FnFactory<A>>>,
    msgs: unsync::UnboundedReceiver<ContextProtocol<A>>,
    sync_msgs: sync::UnboundedReceiver<Envelope<A>>,
    msg: Option<ContextProtocol<A>>,
    sync_msg: Option<Envelope<A>>,
    sync_alive: bool,
}

struct ActorCell<A: Supervised> {
    ctx: A::Context,
    addr: unsync::UnboundedSender<ContextProtocol<A>>,
}

impl<A> Supervisor<A> where A: Supervised + Actor<Context=Context<A>>
{
    /// Start new supervised actor. Depends on `lazy` argument actor could be started
    /// immediately or on first incoming message.
    pub fn start<F>(lazy: bool, f: F) -> (Address<A>, SyncAddress<A>)
        where A: Actor<Context=Context<A>>,
              F: FnOnce(&mut A::Context) -> A + 'static
    {
        // create actor
        let (cell, factory) = if !lazy {
            let mut ctx = Context::new(None);
            let addr = ctx.unsync_sender();
            let act = f(&mut ctx);
            ctx.set_actor(act);
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
            sync_alive: true,
        };
        let addr = Address::new(supervisor.msgs.sender());
        let saddr = SyncAddress::new(stx);
        Arbiter::handle().spawn(supervisor);

        (addr, saddr)
    }

    /// Start new supervised actor in arbiter's thread. Depends on `lazy` argument
    /// actor could be started immediately or on first incoming message.
    pub fn start_in<F>(addr: &SyncAddress<Arbiter>, lazy: bool, f: F) -> Option<SyncAddress<A>>
        where A: Actor<Context=Context<A>>,
              F: FnOnce(&mut Context<A>) -> A + Send + 'static
    {
        if addr.connected() {
            let (tx, rx) = sync::unbounded();

            addr.send(Execute::new(move || -> Result<(), ()> {
                // create actor
                let (cell, factory) = if lazy {
                    let mut ctx = Context::new(None);
                    let addr = ctx.unsync_sender();
                    let act = f(&mut ctx);
                    ctx.set_actor(act);
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
                    sync_alive: true,
                };
                Arbiter::handle().spawn(supervisor);
                Ok(())
            }));

            if addr.connected() {
                Some(SyncAddress::new(tx))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn get_cell(&mut self) -> &mut ActorCell<A> {
        if self.cell.is_none() {
            let f = self.factory.take().expect("Should be available");
            let mut ctx = Context::new(None);

            let addr = ctx.unsync_sender();
            let act = f.call(&mut ctx);
            ctx.set_actor(act);
            self.cell = Some(ActorCell {ctx: ctx, addr: addr});
        }
        self.cell.as_mut().unwrap()
    }

    fn restart(&mut self) {
        let cell = self.cell.take().unwrap();
        let mut ctx = Context::new(None);

        let addr = ctx.unsync_sender();
        ctx.set_actor(cell.ctx.into_inner());
        ctx.restarting();
        self.cell = Some(ActorCell {ctx: ctx, addr: addr});
    }
}

#[doc(hidden)]
impl<A> Future for Supervisor<A> where A: Supervised + Actor<Context=Context<A>>
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
                    ContextProtocol::Upgrade(_) => (),
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
                            ContextProtocol::Upgrade(tx) => {
                                self.sync_alive = true;
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
                    Ok(Async::NotReady) => (),
                    Ok(Async::Ready(None)) | Err(_) => {
                        self.sync_alive = false
                    }
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
                    // supervisor disconnected
                    if !self.get_cell().ctx.alive() &&
                        !self.msgs.connected() &&
                        !self.sync_alive
                    {
                        return Ok(Async::Ready(()))
                    }
                    self.restart();
                }
            }
        }
    }
}

trait FnFactory<A: Actor>: 'static where A::Context: AsyncContext<A> {
    fn call(self: Box<Self>, &mut A::Context) -> A;
}

impl<A: Actor, F: FnOnce(&mut A::Context) -> A + 'static> FnFactory<A> for F
    where A::Context: AsyncContext<A>
{
    #[cfg_attr(feature="cargo-clippy", allow(boxed_local))]
    fn call(self: Box<Self>, ctx: &mut A::Context) -> A {
        (*self)(ctx)
    }
}
