//! Actix system messages

use actor::Actor;
use address::Addr;
use context::Context;
use handler::Message;

/// Message to stop arbiter execution
pub struct StopArbiter(pub i32);

impl Message for StopArbiter {
    type Result = ();
}

/// Start actor in arbiter's thread
pub struct StartActor<A: Actor>(Box<FnBox<A>>);

impl<A: Actor> Message for StartActor<A> {
    type Result = Addr<A>;
}

impl<A: Actor<Context = Context<A>>> StartActor<A> {
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(&mut Context<A>) -> A + Send + 'static,
    {
        Self(Box::new(|| {
            let mut ctx = Context::new();
            let act = f(&mut ctx);
            ctx.run(act)
        }))
    }

    pub(crate) fn call(self) -> Addr<A> {
        self.0.call_box()
    }
}

trait FnBox<A: Actor>: Send + 'static {
    fn call_box(self: Box<Self>) -> Addr<A>;
}

impl<A: Actor, F: FnOnce() -> Addr<A> + Send + 'static> FnBox<A> for F {
    #[cfg_attr(feature = "cargo-clippy", allow(boxed_local))]
    fn call_box(self: Box<Self>) -> Addr<A> {
        (*self)()
    }
}

/// Message to execute a function in an arbiter's thread.
///
/// The arbiter actor handles `Execute` messages.
///
/// # Example
///
/// ```rust
/// # extern crate actix;
/// use actix::prelude::*;
///
/// struct MyActor {
///     addr: Addr<Arbiter>,
/// }
///
/// impl Actor for MyActor {
///     type Context = Context<Self>;
///
///     fn started(&mut self, ctx: &mut Context<Self>) {
///         self.addr
///             .do_send(actix::msgs::Execute::new(|| -> Result<(), ()> {
///                 // do something
///                 // ...
///                 Ok(())
///             }));
///     }
/// }
/// fn main() {}
/// ```
pub struct Execute<I: Send + 'static = (), E: Send + 'static = ()>(Box<FnExec<I, E>>);

/// Execute message response
impl<I: Send, E: Send> Message for Execute<I, E> {
    type Result = Result<I, E>;
}

impl<I, E> Execute<I, E>
where
    I: Send + 'static,
    E: Send + 'static,
{
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce() -> Result<I, E> + Send + 'static,
    {
        Self(Box::new(f))
    }

    /// Execute enclosed function
    pub fn exec(self) -> Result<I, E> {
        self.0.call_box()
    }
}

trait FnExec<I: Send + 'static, E: Send + 'static>: Send + 'static {
    fn call_box(self: Box<Self>) -> Result<I, E>;
}

impl<I, E, F> FnExec<I, E> for F
where
    I: Send + 'static,
    E: Send + 'static,
    F: FnOnce() -> Result<I, E> + Send + 'static,
{
    #[cfg_attr(feature = "cargo-clippy", allow(boxed_local))]
    fn call_box(self: Box<Self>) -> Result<I, E> {
        (*self)()
    }
}
