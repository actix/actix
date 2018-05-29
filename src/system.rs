use futures::sync::oneshot::{channel, Receiver, Sender};
use futures::{future, Future};
use std::collections::HashMap;
use tokio::runtime::current_thread::Runtime;

use actor::Actor;
use address::Addr;
use arbiter::Arbiter;
use context::Context;
use handler::{Handler, Message};
use msgs::{StopArbiter, SystemExit};

/// System is an actor which manages runtime.
///
/// Before starting any actix's actors, `System` actor has to be created and
/// started with `System::run()` call. This method creates new `Arbiter` in
/// current thread and starts `System` actor.
///
/// # Examples
///
/// ```rust
/// extern crate actix;
///
/// use actix::prelude::*;
/// use std::time::Duration;
///
/// struct Timer {
///     dur: Duration,
/// }
///
/// impl Actor for Timer {
///     type Context = Context<Self>;
///
///     // stop system after `self.dur` seconds
///     fn started(&mut self, ctx: &mut Context<Self>) {
///         ctx.run_later(self.dur, |act, ctx| {
///             // send `SystemExit` to `System` actor.
///             Arbiter::system().do_send(actix::msgs::SystemExit(0));
///         });
///     }
/// }
///
/// fn main() {
///     // initialize system and run it.
///     // This function blocks current thread
///     let code = System::run(|| {
///         // Start `Timer` actor
///         Timer {
///             dur: Duration::new(0, 1),
///         }.start();
///     });
///
///     std::process::exit(code);
/// }
/// ```
pub struct System {
    stop: Option<Sender<i32>>,
    arbiters: HashMap<String, Addr<Arbiter>>,
}

impl Actor for System {
    type Context = Context<Self>;
}

impl System {
    #[cfg_attr(feature = "cargo-clippy", allow(new_ret_no_self))]
    /// Create new system
    pub fn new<T: Into<String>>(name: T) -> SystemRuntime {
        let name = name.into();

        let (stop_tx, stop) = channel();

        // start system
        let sys = System {
            arbiters: HashMap::new(),
            stop: Some(stop_tx),
        };

        let mut rt = Runtime::new().unwrap();
        let _ = rt.block_on(future::lazy(move || {
            let addr = sys.start();
            Arbiter::new_system(addr, name);
            Ok::<_, ()>(())
        }));

        SystemRuntime { rt, stop }
    }

    /// This function will start tokio runtime and will finish once the
    /// `SystemExit` message get received.
    /// Function `f` get called within tokio runtime context.
    pub fn run<F>(f: F) -> i32
    where
        F: FnOnce(),
    {
        let (stop_tx, stop) = channel();

        // start system
        let sys = System {
            arbiters: HashMap::new(),
            stop: Some(stop_tx),
        };

        let mut rt = Runtime::new().unwrap();
        let _ = rt.block_on(future::lazy(move || {
            let addr = sys.start();
            Arbiter::new_system(addr, "actix".to_owned());
            Ok::<_, ()>(())
        }));

        // run loop
        let _ = rt.block_on(future::lazy(move || {
            f();
            Ok::<_, ()>(())
        }));
        match rt.block_on(stop) {
            Ok(code) => code,
            Err(_) => 1,
        }
    }
}

/// Helper object that runs System's event loop
#[must_use = "SystemRunner must be run"]
pub struct SystemRuntime {
    rt: Runtime,
    stop: Receiver<i32>,
}

impl SystemRuntime {
    /// This function will start event loop and will finish once the
    /// `SystemExit` message get received.
    pub fn run<F>(self, f: F) -> i32
    where
        F: FnOnce(),
    {
        let SystemRuntime { mut rt, stop, .. } = self;

        // run loop
        let _ = rt.block_on(future::lazy(move || {
            f();
            Ok::<_, ()>(())
        }));
        match rt.block_on(stop) {
            Ok(code) => code,
            Err(_) => 1,
        }
    }

    pub fn run_until_complete<F, I, E>(&mut self, fut: F) -> Result<I, E>
    where
        F: Future<Item = I, Error = E>,
    {
        self.rt.block_on(fut)
    }
}

impl Handler<SystemExit> for System {
    type Result = ();

    fn handle(&mut self, msg: SystemExit, _: &mut Context<Self>) {
        // stop arbiters
        for addr in self.arbiters.values() {
            addr.do_send(StopArbiter(msg.0));
        }
        // stop event loop
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(msg.0);
        }
    }
}

/// Register Arbiter within system
pub(crate) struct RegisterArbiter(pub String, pub Addr<Arbiter>);

#[doc(hidden)]
impl Message for RegisterArbiter {
    type Result = ();
}

#[doc(hidden)]
impl Handler<RegisterArbiter> for System {
    type Result = ();

    fn handle(&mut self, msg: RegisterArbiter, _: &mut Context<Self>) {
        self.arbiters.insert(msg.0, msg.1);
    }
}

/// Unregister Arbiter
pub(crate) struct UnregisterArbiter(pub String);

#[doc(hidden)]
impl Message for UnregisterArbiter {
    type Result = ();
}

#[doc(hidden)]
impl Handler<UnregisterArbiter> for System {
    type Result = ();

    fn handle(&mut self, msg: UnregisterArbiter, _: &mut Context<Self>) {
        self.arbiters.remove(&msg.0);
    }
}
