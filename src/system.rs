use std::cell::RefCell;
use std::collections::HashMap;

use futures::sync::oneshot::{channel, Receiver, Sender};
use futures::{future, Future};
use tokio::runtime::current_thread::Runtime;

use actor::Actor;
use address::{channel as addr_channel, Addr};
use arbiter::Arbiter;
use context::Context;
use handler::{Handler, Message};
use msgs::{Execute, StopArbiter};
use registry::SystemRegistry;

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
///             // Stop current running system.
///             System::current().stop();
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
#[derive(Clone)]
pub struct System {
    sys: Addr<SystemArbiter>,
    arbiter: Addr<Arbiter>,
    registry: SystemRegistry,
}

thread_local!(static CURRENT: RefCell<Option<System>> = RefCell::new(None););

impl System {
    #[cfg_attr(feature = "cargo-clippy", allow(new_ret_no_self))]
    /// Create new system.
    ///
    /// This method panics if it can not create tokio runtime
    pub fn new<T: Into<String>>(name: T) -> SystemRunner {
        System::create_runtime(name.into(), || {})
    }

    /// Get current running system.
    pub fn current() -> System {
        CURRENT.with(|cell| match *cell.borrow() {
            Some(ref sys) => sys.clone(),
            None => panic!("System is not running"),
        })
    }

    /// Set current running system.
    #[doc(hidden)]
    pub(crate) fn is_set() -> bool {
        CURRENT.with(|cell| cell.borrow().is_some())
    }

    /// Set current running system.
    #[doc(hidden)]
    pub fn set_current(sys: System) {
        CURRENT.with(|s| {
            *s.borrow_mut() = Some(sys);
        })
    }

    /// Execute function with system reference.
    pub fn with_current<F, R>(f: F) -> R
    where
        F: FnOnce(&System) -> R,
    {
        CURRENT.with(|cell| match *cell.borrow() {
            Some(ref sys) => f(sys),
            None => panic!("System is not running"),
        })
    }

    /// Stop the system
    pub fn stop(&self) {
        self.sys.do_send(SystemExit(0));
    }

    pub(crate) fn sys(&self) -> &Addr<SystemArbiter> {
        &self.sys
    }

    /// System arbiter
    pub fn arbiter(&self) -> &Addr<Arbiter> {
        &self.arbiter
    }

    /// Get current system registry.
    pub fn registry(&self) -> &SystemRegistry {
        &self.registry
    }

    /// This function will start tokio runtime and will finish once the
    /// `System::stop()` message get called.
    /// Function `f` get called within tokio runtime context.
    pub fn run<F>(f: F) -> i32
    where
        F: FnOnce() + 'static,
    {
        System::create_runtime("actix".to_owned(), f).run()
    }

    fn create_runtime<F>(name: String, f: F) -> SystemRunner
    where
        F: FnOnce() + 'static,
    {
        let (stop_tx, stop) = channel();
        let (addr_sender, addr_receiver) = addr_channel::channel(16);
        let (arb_sender, arb_receiver) = addr_channel::channel(16);

        let addr = Addr::new(arb_sender);
        let system = System {
            sys: Addr::new(addr_sender),
            arbiter: addr.clone(),
            registry: SystemRegistry::new(addr),
        };
        System::set_current(system.clone());

        // system arbiter
        let arb = SystemArbiter {
            arbiters: HashMap::new(),
            stop: Some(stop_tx),
        };

        let mut rt = Runtime::new().unwrap();

        // init system arbiter and run configuration method
        let _ = rt.block_on(future::lazy(move || {
            Arbiter::new_system(arb_receiver, name);
            let ctx = Context::with_receiver(addr_receiver);
            ctx.run(arb);

            f();
            Ok::<_, ()>(())
        }));

        SystemRunner { rt, stop }
    }
}

/// Helper object that runs System's event loop
#[must_use = "SystemRunner must be run"]
pub struct SystemRunner {
    rt: Runtime,
    stop: Receiver<i32>,
}

impl SystemRunner {
    /// This function will start event loop and will finish once the
    /// `System::stop()` function get called.
    pub fn run(self) -> i32 {
        let SystemRunner { mut rt, stop, .. } = self;

        // run loop
        let _ = rt.block_on(future::lazy(move || {
            Arbiter::run_system();
            Ok::<_, ()>(())
        }));
        let code = match rt.block_on(stop) {
            Ok(code) => code,
            Err(_) => 1,
        };
        Arbiter::stop_system();
        code
    }

    /// Execute a future and wait for result.
    pub fn block_on<F, I, E>(&mut self, fut: F) -> Result<I, E>
    where
        F: Future<Item = I, Error = E>,
    {
        let _ = self.rt.block_on(future::lazy(move || {
            Arbiter::run_system();
            Ok::<_, ()>(())
        }));
        let res = self.rt.block_on(fut);
        let _ = self.rt.block_on(future::lazy(move || {
            Arbiter::stop_system();
            Ok::<_, ()>(())
        }));
        res
    }
}

pub(crate) struct SystemArbiter {
    stop: Option<Sender<i32>>,
    arbiters: HashMap<String, Addr<Arbiter>>,
}

impl Actor for SystemArbiter {
    type Context = Context<Self>;
}

/// Stop system execution
struct SystemExit(pub i32);

impl Message for SystemExit {
    type Result = ();
}

impl Handler<SystemExit> for SystemArbiter {
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
impl Handler<RegisterArbiter> for SystemArbiter {
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
impl Handler<UnregisterArbiter> for SystemArbiter {
    type Result = ();

    fn handle(&mut self, msg: UnregisterArbiter, _: &mut Context<Self>) {
        self.arbiters.remove(&msg.0);
    }
}

/// Execute function in arbiter's thread
impl<I: Send, E: Send> Handler<Execute<I, E>> for SystemArbiter {
    type Result = Result<I, E>;

    fn handle(&mut self, msg: Execute<I, E>, _: &mut Context<Self>) -> Result<I, E> {
        msg.exec()
    }
}
