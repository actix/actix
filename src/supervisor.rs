use futures::{Future, Async, Poll};

use actor::{Actor, Supervised};
use arbiter::Arbiter;
use address::{sync_channel, ActorAddress, Addr, Syn};
use context::Context;
use mailbox::DEFAULT_CAPACITY;
use msgs::Execute;

/// Actor supervisor
///
/// Supervisor manages incoming message for actor. In case of actor failure, supervisor
/// creates new execution context and restarts actor lifecycle. Supervisor does not
/// does not re-create actor, it just calls `restarting()` method.
///
/// Supervisor has same livecycle as actor. In situation when all addresses to supervisor
/// get dropped and actor does not execute anything, supervisor terminates.
///
/// `Supervisor` can not guarantee that actor successfully process incoming message.
/// If actor fails during message processing, this message can not be recovered. Sender
/// would receive `Err(Cancelled)` error in this situation.
///
/// ## Example
///
/// ```rust
/// # #[macro_use] extern crate actix;
/// # use actix::prelude::*;
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
///     let addr: Addr<Unsync<_>> = actix::Supervisor::start(|_| MyActor);
///
///     addr.send(Die);
///     sys.run();
/// }
/// ```
pub struct Supervisor<A> where A: Supervised + Actor<Context=Context<A>> {
    ctx: A::Context
}

impl<A> Supervisor<A> where A: Supervised + Actor<Context=Context<A>>
{
    /// Start new supervised actor in current Arbiter.
    ///
    /// Type of returned address depends on variable type. For example to get `Addr<Syn<_>>`
    /// of newly created actor, use explicitly `Addr<Syn<_>>` type as type of a variable.
    ///
    /// ```rust
    /// # #[macro_use] extern crate actix;
    /// # use actix::prelude::*;
    /// struct MyActor;
    ///
    /// impl Actor for MyActor {
    ///    type Context = Context<Self>;
    /// }
    ///
    /// # impl actix::Supervised for MyActor {}
    /// # fn main() {
    /// #    let sys = System::new("test");
    /// // Get `Addr<Unsync<_>>` of a MyActor actor
    /// let addr1: Addr<Unsync<_>> = actix::Supervisor::start(|_| MyActor);
    ///
    /// // Get `Addr<Syn<_>>` of a MyActor actor
    /// let addr2: Addr<Syn<_>> = actix::Supervisor::start(|_| MyActor);
    /// # }
    /// ```
    pub fn start<Addr, F>(f: F) -> Addr
        where F: FnOnce(&mut A::Context) -> A + 'static,
              A: Actor<Context=Context<A>> + ActorAddress<A, Addr>
    {
        // create actor
        let mut ctx = Context::new(None);
        let act = f(&mut ctx);
        let addr =  <A as ActorAddress<A, Addr>>::get(&mut ctx);
        ctx.set_actor(act);

        // create supervisor
        Arbiter::handle().spawn(Supervisor::<A>{ctx: ctx});

        addr
    }

    /// Start new supervised actor in arbiter's thread.
    pub fn start_in<F>(addr: &Addr<Syn<Arbiter>>, f: F) -> Addr<Syn<A>>
        where A: Actor<Context=Context<A>>,
              F: FnOnce(&mut Context<A>) -> A + Send + 'static
    {
        let (tx, rx) = sync_channel::channel(DEFAULT_CAPACITY);

        addr.send(Execute::new(move || -> Result<(), ()> {
            let mut ctx = Context::with_receiver(None, rx);
            let act = f(&mut ctx);
            ctx.set_actor(act);
            Arbiter::handle().spawn(Supervisor::<A>{ctx: ctx});
            Ok(())
        }));

        Addr::new(tx)
    }
}

#[doc(hidden)]
impl<A> Future for Supervisor<A> where A: Supervised + Actor<Context=Context<A>> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.ctx.poll() {
                Ok(Async::NotReady) =>
                    return Ok(Async::NotReady),
                Ok(Async::Ready(_)) | Err(_) => {
                    // stop if context's address is not connected
                    if !self.ctx.restart() {
                        return Ok(Async::Ready(()))
                    }
                }
            }
        }
    }
}
