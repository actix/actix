//! Actix system messages

use actor::{Actor, AsyncContext};
use address::Addr;
use arbiter::Arbiter;
use context::Context;
use handler::Message;

/// Stop arbiter execution
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
        StartActor(Box::new(|| {
            let ctx = Context::create(f);
            let addr = ctx.address();
            Arbiter::spawn(ctx);
            addr
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

/// Execute function in arbiter's thread
///
/// Arbiter` actor handles Execute message.
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
        Execute(Box::new(f))
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
