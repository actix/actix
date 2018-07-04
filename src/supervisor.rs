use futures::{Async, Future, Poll};

use actor::{Actor, AsyncContext, Supervised};
use address::{channel, Addr};
use arbiter::Arbiter;
use context::Context;
use contextimpl::ContextFut;
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
/// #       System::current().stop();
///     }
/// }
///
/// fn main() {
///     System::run(|| {
///         let addr = actix::Supervisor::start(|_| MyActor);
///
///         addr.do_send(Die);
///     });
/// }
/// ```
pub struct Supervisor<A>
where
    A: Supervised + Actor<Context = Context<A>>,
{
    fut: ContextFut<A, Context<A>>,
}

impl<A> Supervisor<A>
where
    A: Supervised + Actor<Context = Context<A>>,
{
    /// Start new supervised actor in current tokio runtime.
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
    /// #         System::current().stop();
    /// # });}
    /// ```
    pub fn start<F>(f: F) -> Addr<A>
    where
        F: FnOnce(&mut A::Context) -> A + 'static,
        A: Actor<Context = Context<A>>,
    {
        // create actor
        let mut ctx = Context::new();
        let act = f(&mut ctx);
        let addr = ctx.address();
        let fut = ctx.into_future(act);

        // create supervisor
        Arbiter::spawn(Supervisor::<A> { fut });

        addr
    }

    /// Start new supervised actor in arbiter's thread.
    pub fn start_in_arbiter<F>(sys: &Addr<Arbiter>, f: F) -> Addr<A>
    where
        A: Actor<Context = Context<A>>,
        F: FnOnce(&mut Context<A>) -> A + Send + 'static,
    {
        let (tx, rx) = channel::channel(DEFAULT_CAPACITY);

        sys.do_send(Execute::new(move || -> Result<(), ()> {
            let mut ctx = Context::with_receiver(rx);
            let act = f(&mut ctx);
            let fut = ctx.into_future(act);

            Arbiter::spawn(Supervisor::<A> { fut });
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
            match self.fut.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(_)) | Err(_) => {
                    // stop if context's address is not connected
                    if !self.fut.restart() {
                        return Ok(Async::Ready(()));
                    }
                }
            }
        }
    }
}
