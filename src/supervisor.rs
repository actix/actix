use futures::{Async, Future, Poll};
use tokio;

use actor::{Actor, AsyncContext, Ctx, Supervised};
use address::{channel, Addr};
use arbiter::Arbiter;
use context::Context;
use mailbox::DEFAULT_CAPACITY;
use msgs::Execute;

/// Actor supervisor
///
/// Supervisor manages incoming message for actor. In case of actor failure,
/// supervisor creates new execution context and restarts actor lifecycle.
/// Supervisor does not does not re-create actor, it just calls `restarting()`
/// method.
///
/// Supervisor has same lifecycle as actor. In situation when all addresses to
/// supervisor get dropped and actor does not execute anything, supervisor
/// terminates.
///
/// `Supervisor` can not guarantee that actor successfully process incoming
/// message. If actor fails during message processing, this message can not be
/// recovered. Sender would receive `Err(Cancelled)` error in this situation.
///
/// ## Example
///
/// ```rust
/// # #[macro_use] extern crate actix;
/// # use actix::prelude::*;
/// #[derive(Message)]
/// struct Die;
///
/// struct MyActor{ctx: Ctx<MyActor>}
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
///     fn handle(&mut self, _: Die) {
///         self.ctx.stop();
/// #       Arbiter::system().do_send(actix::msgs::SystemExit(0));
///     }
/// }
///
/// fn main() {
///     System::run(|| {
///         let addr = actix::Supervisor::start(|ctx| MyActor{ctx});
///
///         addr.do_send(Die);
///     });
/// }
/// ```
pub struct Supervisor<A>
where
    A: Supervised + Actor<Context = Context<A>>,
{
    ctx: A::Context,
}

impl<A> Supervisor<A>
where
    A: Supervised + Actor<Context = Context<A>> + Send,
{
    /// Start new supervised actor in current Arbiter.
    ///
    /// Type of returned address depends on variable type. For example to get
    /// `Addr<Syn, _>` of newly created actor, use explicitly `Addr<Syn,
    /// _>` type as type of a variable.
    ///
    /// ```rust
    /// # #[macro_use] extern crate actix;
    /// # use actix::prelude::*;
    /// struct MyActor;
    ///
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    /// }
    ///
    /// # impl actix::Supervised for MyActor {}
    /// # fn main() {
    /// #    System::run(|| {
    /// // Get `Addr` of a MyActor actor
    /// let addr = actix::Supervisor::start(|_| MyActor);
    /// #         Arbiter::system().do_send(actix::msgs::SystemExit(0));
    /// # });}
    /// ```
    pub fn start<F>(f: F) -> Addr<A>
    where
        F: FnOnce(Ctx<A>) -> A + 'static,
        A: Actor<Context = Context<A>>,
    {
        // create actor
        let mut ctx = Context::new(None);
        let link = Ctx::new(ctx.clone());
        let act = f(link);
        let addr = ctx.address();
        ctx.set_actor(act);

        // create supervisor
        tokio::spawn(Supervisor::<A> { ctx });

        addr
    }

    /// Start new supervised actor in arbiter's thread.
    pub fn start_in<F>(addr: &Addr<Arbiter>, f: F) -> Addr<A>
    where
        A: Actor<Context = Context<A>> + Send,
        F: FnOnce(&mut Context<A>) -> A + Send + 'static,
    {
        let (tx, rx) = channel::channel(DEFAULT_CAPACITY);

        addr.do_send(Execute::new(move || -> Result<(), ()> {
            let mut ctx = Context::with_receiver(None, rx);
            let act = f(&mut ctx);
            ctx.set_actor(act);
            tokio::spawn(Supervisor::<A> { ctx });
            Ok(())
        }));

        Addr::new(tx)
    }
}

#[doc(hidden)]
impl<A> Future for Supervisor<A>
where
    A: Supervised + Actor<Context = Context<A>>,
{
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.ctx.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(_)) | Err(_) => {
                    // stop if context's address is not connected
                    if !self.ctx.restart() {
                        return Ok(Async::Ready(()));
                    }
                }
            }
        }
    }
}
