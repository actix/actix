use std::collections::HashMap;
use tokio_core::reactor::{Core, Handle};
use futures::sync::oneshot::{channel, Receiver, Sender};

use address::SyncAddress;
use arbiter::{Arbiter, StartActor, StopArbiter};
use builder::ActorBuilder;
use context::Context;
use message::{MessageFuture, MessageFutureResult};
use actor::{Actor, MessageHandler, MessageResponse};

/// System is an actor which manages process.
///
/// Before starting any actix's actors, `System` actor has to be create
/// with `System::new()` call. This method creates new `Arbiter` in current thread
/// and starts `System` actor.
///
/// # Examples
///
/// ```rust
/// extern crate actix;
/// extern crate tokio_core;
///
/// use std::time::Duration;
/// use tokio_core::reactor::Timeout;
/// use actix::prelude::*;
///
/// struct Timer {dur: Duration}
///
/// impl Actor for Timer {
///
///    // stop system in `self.dur` seconds
///    fn started(&mut self, ctx: &mut Context<Self>) {
///        Timeout::new(self.dur, Arbiter::handle())
///           .unwrap()
///           .actfuture()
///           .then(|_, srv: &mut Timer, ctx: &mut Context<Self>| {
///               // send `SystemExit` to `System` actor.
///               Arbiter::system().send(actix::SystemExit(0));
///               fut::ok(())
///           })
///           .spawn(ctx);
///    }
/// }
///
/// fn main() {
///    // initialize system
///    let sys = System::new("test".to_owned());
///
///    // Start `Timer` actor
///    let _:() = Timer{dur: Duration::new(0, 1)}.start();
///
///    // Run system, this function blocks current thread
///    let code = sys.run();
///    std::process::exit(code);
/// }
/// ```
pub struct System {
    stop: Option<Sender<i32>>,
    arbiters: HashMap<String, SyncAddress<Arbiter>>,
}

impl Actor for System {}

impl System {

    #[cfg_attr(feature="cargo-clippy", allow(new_ret_no_self))]
    /// Create new system
    pub fn new(name: String) -> SystemRunner {
        let core = Arbiter::new_system();
        let (stop_tx, stop_rx) = channel();

        // start system
        let sys = System {
            arbiters: HashMap::new(), stop: Some(stop_tx)}.start();
        Arbiter::set_system(sys, name);

        SystemRunner {
            core: core,
            stop: stop_rx,
        }
    }
}

#[must_use="SystemRunner must be run"]
pub struct SystemRunner {
    core: Core,
    stop: Receiver<i32>,
}

impl SystemRunner {

    /// Returns handle to current event loop.
    pub fn handle(&self) -> &Handle {
        Arbiter::handle()
    }

    /// This function will start tokio event loop and will finish once the `SystemExit`
    /// message get received.
    pub fn run(self) -> i32 {
        let SystemRunner { mut core, stop, ..} = self;

        // run loop
        match core.run(stop) {
            Ok(code) => code,
            Err(_) => 1,
        }
    }
}

/// Stop system execution
pub struct SystemExit(pub i32);

#[doc(hidden)]
impl MessageResponse<SystemExit> for System {
    type Item = ();
    type Error = ();
}

impl MessageHandler<SystemExit> for System {

    fn handle(&mut self, msg: SystemExit, _: &mut Context<Self>)
              -> MessageFuture<Self, SystemExit>
    {
        // stop rbiters
        for addr in self.arbiters.values() {
            addr.send(StopArbiter(msg.0));
        }
        // stop event loop
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(msg.0);
        }
        ().to_result()
    }
}

/// Register Arbiter within system
pub(crate) struct RegisterArbiter(pub String, pub SyncAddress<Arbiter>);

#[doc(hidden)]
impl MessageResponse<RegisterArbiter> for System {
    type Item = ();
    type Error = ();
}

#[doc(hidden)]
impl MessageHandler<RegisterArbiter> for System {

    fn handle(&mut self, msg: RegisterArbiter, _: &mut Context<Self>)
              -> MessageFuture<Self, RegisterArbiter>
    {
        self.arbiters.insert(msg.0, msg.1);
        ().to_result()
    }
}

/// Unregister Arbiter
pub(crate) struct UnregisterArbiter(pub String);

#[doc(hidden)]
impl MessageResponse<UnregisterArbiter> for System {
    type Item = ();
    type Error = ();
}

#[doc(hidden)]
impl MessageHandler<UnregisterArbiter> for System {

    fn handle(&mut self, msg: UnregisterArbiter, _: &mut Context<Self>)
              -> MessageFuture<Self, UnregisterArbiter>
    {
        self.arbiters.remove(&msg.0);
        ().to_result()
    }
}

impl<A> MessageResponse<StartActor<A>> for System where A: Actor {
    type Item = SyncAddress<A>;
    type Error = ();
}

/// Start actor in System's context
impl<A> MessageHandler<StartActor<A>> for System where A: Actor {

    fn handle(&mut self, msg: StartActor<A>, _: &mut Context<Self>)
              -> MessageFuture<Self, StartActor<A>>
    {
        msg.call().to_result()
    }
}
