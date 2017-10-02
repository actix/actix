//! Actix system messages

use actor::Actor;
use address::SyncAddress;
use context::Context;
use builder::ActorBuilder;

/// Stop system execution
pub struct SystemExit(pub i32);

/// Stop arbiter execution
pub struct StopArbiter(pub i32);

/// Start actor in arbiter's thread
pub struct StartActor<A: Actor>(Box<FnBox<A>>);

impl<A: Actor<Context=Context<A>>> StartActor<A>
{
    pub fn new<F>(f: F) -> Self
        where F: FnOnce(&mut Context<A>) -> A + Send + 'static
    {
        StartActor(Box::new(|| A::create(f)))
    }

    pub(crate) fn call(self) -> SyncAddress<A> {
        self.0.call_box()
    }
}

trait FnBox<A: Actor>: Send + 'static {
    fn call_box(self: Box<Self>) -> SyncAddress<A>;
}

impl<A: Actor, F: FnOnce() -> SyncAddress<A> + Send + 'static> FnBox<A> for F {
    #[cfg_attr(feature="cargo-clippy", allow(boxed_local))]
    fn call_box(self: Box<Self>) -> SyncAddress<A> {
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
/// extern crate actix;
///
/// use actix::prelude::*;
///
/// struct MyActor{addr: SyncAddress<Arbiter>}
///
/// impl Actor for MyActor {
///    type Context = Context<Self>;
///
///    fn started(&mut self, ctx: &mut Context<Self>) {
///        self.addr.send(msgs::Execute::new(|| -> Result<(), ()> {
///            // do something
///            // ...
///            Ok(())
///        }));
///    }
/// }
/// fn main() {}
/// ```
pub struct Execute<I: Send + 'static = (), E: Send + 'static = ()>(Box<FnExec<I, E>>);

impl<I, E> Execute<I, E>
    where I: Send + 'static, E: Send + 'static
{
    pub fn new<F>(f: F) -> Self where F: FnOnce() -> Result<I, E> + Send + 'static
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
    where I: Send + 'static,
          E: Send + 'static,
          F: FnOnce() -> Result<I, E> + Send + 'static
{
    #[cfg_attr(feature="cargo-clippy", allow(boxed_local))]
    fn call_box(self: Box<Self>) -> Result<I, E> {
        (*self)()
    }
}
