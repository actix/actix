use std::collections::HashMap;

use futures::sync::oneshot::{channel, Receiver, Sender};
use futures::{future, Future};
use tokio::runtime::{Builder, Runtime};
use tokio_threadpool::Builder as ThreadPoolBuilder;

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
    /// Create new system.
    ///
    /// This method panics if it can not create tokio runtime
    pub fn new<T: Into<String>>(name: T) -> SystemRuntime {
        let (rt, stop) =
            System::create_runtime(&format!("{}-thread-pool-", name.into()), || {});

        SystemRuntime { rt, stop }
    }

    /// This function will start tokio runtime and will finish once the
    /// `SystemExit` message get received.
    /// Function `f` get called within tokio runtime context.
    pub fn run<F>(f: F) -> i32
    where
        F: FnOnce() + Send + 'static,
    {
        let (_rt, stop) = System::create_runtime("actix-thread-pool-", f);

        match stop.wait() {
            Ok(code) => code,
            Err(_) => 1,
        }
    }

    fn create_runtime<F>(name: &str, f: F) -> (Runtime, Receiver<i32>)
    where
        F: FnOnce() + Send + 'static,
    {
        let (stop_tx, stop) = channel();

        // start system
        let sys = System {
            arbiters: HashMap::new(),
            stop: Some(stop_tx),
        };

        let saddr = Arbiter::system_ref();
        let system = saddr.clone();

        let sarb = Arbiter::system_arb_ref();
        let sarb2 = sarb.clone();

        let reg = Arbiter::system_reg();

        let mut threadpool = ThreadPoolBuilder::new();
        threadpool.name_prefix(name).after_start(move || {
            Arbiter::set_system_ref(saddr.clone());
            Arbiter::set_system_arb_ref(sarb2.clone());
            Arbiter::set_system_reg(reg.clone());
        });

        let mut rt = Builder::new()
            .threadpool_builder(threadpool)
            .build()
            .unwrap();

        // init system arbiter and run configuration method
        let (tx, rx) = channel();
        let _ = rt.spawn(future::lazy(move || {
            let addr = sys.start();
            *system.lock().unwrap() = Some(addr);

            let addr = Arbiter::new_system();
            *sarb.lock().unwrap() = Some(addr);

            f();
            let _ = tx.send(());
            Ok::<_, ()>(())
        }));
        let _ = rx.wait();

        (rt, stop)
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
    pub fn run(self) -> i32 {
        // run loop
        match self.stop.wait() {
            Ok(code) => code,
            Err(_) => 1,
        }
    }

    pub fn config<F>(mut self, f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        // run config fn
        let (tx, rx) = channel();
        self.rt.spawn(future::lazy(move || {
            f();
            let _ = tx.send(());
            Ok::<_, ()>(())
        }));
        let _ = rx.wait();

        self
    }
}

impl Handler<SystemExit> for System {
    type Result = ();

    fn handle(&mut self, msg: SystemExit) {
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

    fn handle(&mut self, msg: RegisterArbiter) {
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

    fn handle(&mut self, msg: UnregisterArbiter) {
        self.arbiters.remove(&msg.0);
    }
}
