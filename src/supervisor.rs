use std::pin::Pin;
use std::task::{self, Poll};

use actix_rt::Arbiter;
use futures::Future;
use pin_project::pin_project;

use crate::actor::{Actor, AsyncContext, Supervised};
use crate::address::{channel, Addr};
use crate::context::Context;
use crate::contextimpl::ContextFut;
use crate::mailbox::DEFAULT_CAPACITY;

/// Actor supervisor
///
/// Supervisor manages incoming message for actor. In case of actor failure,
/// supervisor creates new execution context and restarts actor lifecycle.
/// Supervisor does not re-create actor, it just calls `restarting()`
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
/// # use actix::prelude::*;
/// #[derive(Message)]
/// #[rtype(result = "()")]
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
#[pin_project]
#[derive(Debug)]
pub struct Supervisor<A>
where
    A: Supervised + Actor<Context = Context<A>>,
{
    #[pin]
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
        actix_rt::spawn(Self { fut });

        addr
    }

    /// Start new supervised actor in arbiter's thread.
    pub fn start_in_arbiter<F>(sys: &Arbiter, f: F) -> Addr<A>
    where
        A: Actor<Context = Context<A>>,
        F: FnOnce(&mut Context<A>) -> A + Send + 'static,
    {
        let (tx, rx) = channel::channel(DEFAULT_CAPACITY);

        sys.exec_fn(move || {
            let mut ctx = Context::with_receiver(rx);
            let act = f(&mut ctx);
            let fut = ctx.into_future(act);

            actix_rt::spawn(Self { fut });
        });

        Addr::new(tx)
    }
}

#[doc(hidden)]
impl<A> Future for Supervisor<A>
where
    A: Supervised + Actor<Context = Context<A>>,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        loop {
            match this.fut.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(_) => {
                    // stop if context's address is not connected
                    if !this.fut.restart() {
                        return Poll::Ready(());
                    }
                }
            }
        }
    }
}
