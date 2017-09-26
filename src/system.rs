use std;
use std::cell::RefCell;
use tokio_core::reactor::{Core, Handle};
use futures::sync::oneshot::{channel, Receiver, Sender};

use address::SyncAddress;
use arbiter::Arbiter;
use builder::ActorBuilder;
use context::Context;
use message::{MessageFuture, MessageFutureResult};
use actor::{Actor, MessageHandler};

thread_local!(
    static ADDR: RefCell<Option<SyncAddress<System>>> = RefCell::new(None);
);

/// System is an actor which manages process.
///
/// Before starting any actix's actors, `System` actor has to be initialized
/// with `System::init()` call. This method creates new `Arbiter` in current thread
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
///               System::get().send(actix::SystemExit(0));
///               fut::ok(())
///           })
///           .spawn(ctx);
///    }
/// }
///
/// fn main() {
///    // initialize system
///    let sys = System::init();
///
///    // Start `Timer` actor
///    let _:() = Timer{dur: Duration::new(0, 1)}.start();
///
///    // Run system, this function blocks forever
///    sys.run()
/// }
/// ```
#[must_use="System must be run"]
pub struct System {
    core: Option<Core>,
    stop: Option<Receiver<(i32, bool)>>,
    tx: Option<Sender<(i32, bool)>>,
}

impl Actor for System {}

impl System {

    /// This function returns system address,
    /// `get` function panics if `System` is not initialized.
    pub fn get() -> SyncAddress<System> {
        ADDR.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("System is not running"),
        })
    }

    /// Initialize system
    pub fn init() -> System {
        let (stop_tx, stop_rx) = channel();
        let core = Arbiter::new_system();

        // start system
        let sys = System {core: None, tx: Some(stop_tx), stop: None}.start();
        ADDR.with(|cell| {
            *cell.borrow_mut() = Some(sys);
        });

        System {
            core: Some(core),
            stop: Some(stop_rx),
            tx: None,
        }
    }

    /// Returns handle to current event loop. Same as `Arbiter::handle()`
    pub fn handle(&self) -> &Handle {
        Arbiter::handle()
    }

    /// This function will start tokio event loop and will finish once the `SystemExit`
    /// message get received. Once `SystemExit` message get received, process exits
    /// with code encoded in message.
    pub fn run(self) {
        let System { core, stop, ..} = self;

        // run loop
        let (code, exit) = match core.unwrap().run(stop.unwrap()) {
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
        if let Some(stop) = self.tx.take() {
            let _ = stop.send((msg.0, true));
        }
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
    if let Some(stop) = self.tx.take() {
        let _ = stop.send((msg.0, false));
    }
    ().to_result()
}
}
