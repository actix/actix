use std;
use std::collections::HashMap;
use tokio_core::reactor::{Core, Handle};
use futures::sync::oneshot::{channel, Receiver, Sender};

use address::SyncAddress;
use arbiter::{Arbiter, StopArbiter};
use builder::ActorBuilder;
use context::Context;
use message::{MessageFuture, MessageFutureResult};
use actor::{Actor, MessageHandler};

/// System is an actor which manages process.
///
/// Before starting any actix's actors, `System` actor has to be create
/// with `System::new()` call. This method creates new `Arbiter` in current thread
/// and starts `System` actor.
///
/// # Examples
///
/// ```rust
/// #![allow(unused_variables)]
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
///               Arbiter::get_system().send(actix::SystemExit(0));
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
///    sys.run()
/// }
/// ```
pub struct System {
    stop: Option<Sender<(i32, bool)>>,
    arbiters: HashMap<String, SyncAddress<Arbiter>>,
}

impl Actor for System {}

impl System {

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

    fn stop(&mut self, code: i32, exit: bool) {
        for addr in self.arbiters.values() {
            addr.send(StopArbiter(0));
        }
        if let Some(stop) = self.stop.take() {
            let _ = stop.send((code, exit));
        }
    }
}

#[must_use="SystemRunner must be run"]
pub struct SystemRunner {
    core: Core,
    stop: Receiver<(i32, bool)>,
}

impl SystemRunner {

    /// Returns handle to current event loop.
    pub fn handle(&self) -> &Handle {
        Arbiter::handle()
    }

    /// This function will start tokio event loop and will finish once the `SystemExit`
    /// message get received. Once `SystemExit` message get received, process exits
    /// with code encoded in message.
    pub fn run(self) {
        let SystemRunner { mut core, stop, ..} = self;

        // run loop
        let (code, exit) = match core.run(stop) {
            Ok(code) => code,
            Err(_) => (1, true),
        };
        if exit {
            std::process::exit(code);
        }
    }
}

/// Stop system execution and exit process with encoded code.
pub struct SystemExit(pub i32);

/// Stop system execution
pub struct SystemStop(pub i32);

impl MessageHandler<SystemExit> for System {
    type Item = ();
    type Error = ();
    type InputError = ();

    fn handle(&mut self, msg: SystemExit, _: &mut Context<Self>)
              -> MessageFuture<Self, SystemExit>
    {
        self.stop(msg.0, true);
        ().to_result()
    }
}

impl MessageHandler<SystemStop> for System {
    type Item = ();
    type Error = ();
    type InputError = ();

    fn handle(&mut self, msg: SystemStop, _: &mut Context<Self>)
              -> MessageFuture<Self, SystemStop>
    {
        self.stop(msg.0, false);
        ().to_result()
    }
}


/// Register Arbiter within system
pub(crate) struct RegisterArbiter(pub String, pub SyncAddress<Arbiter>);

impl MessageHandler<RegisterArbiter> for System {
    type Item = ();
    type Error = ();
    type InputError = ();

    fn handle(&mut self, msg: RegisterArbiter, _: &mut Context<Self>)
              -> MessageFuture<Self, RegisterArbiter>
    {
        self.arbiters.insert(msg.0, msg.1);
        ().to_result()
    }
}

/// Unregister Arbiter
pub(crate) struct UnregisterArbiter(pub String);

impl MessageHandler<UnregisterArbiter> for System {
    type Item = ();
    type Error = ();
    type InputError = ();

    fn handle(&mut self, msg: UnregisterArbiter, _: &mut Context<Self>)
              -> MessageFuture<Self, UnregisterArbiter>
    {
        self.arbiters.remove(&msg.0);
        ().to_result()
    }
}
