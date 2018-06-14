use std::cell::RefCell;
use std::collections::HashMap;

use futures::sync::oneshot::{channel, Receiver, Sender};
use futures::{future, Future};
use tokio;
use tokio::runtime::{Builder, Runtime};
use tokio_threadpool::Builder as ThreadPoolBuilder;

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
    arbiter: Addr<SystemArbiter>,
    registry: SystemRegistry,
}

thread_local!(static CURRENT: RefCell<Option<System>> = RefCell::new(None););

impl System {
    #[cfg_attr(feature = "cargo-clippy", allow(new_ret_no_self))]
    /// Create new system.
    ///
    /// This method panics if it can not create tokio runtime
    pub fn new<T: Into<String>>(name: T) -> SystemRunner {
        let (rt, stop) =
            System::create_runtime(&format!("{}-thread-pool-", name.into()), 1, || {});

        SystemRunner { rt, stop }
    }

    /// Create new system and set tokio runtime pool size.
    ///
    /// By default pool size is 1. This method panics if it can not create tokio runtime
    pub fn with_pool_size<T: Into<String>>(name: T, size: usize) -> SystemRunner {
        let (rt, stop) = System::create_runtime(
            &format!("{}-thread-pool-", name.into()),
            size,
            || {},
        );

        SystemRunner { rt, stop }
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
        self.arbiter.do_send(SystemExit(0));
    }

    #[doc(hidden)]
    pub fn arbiter(&self) -> &Addr<SystemArbiter> {
        &self.arbiter
    }

    /// Get current system registry.
    pub fn registry(&self) -> &SystemRegistry {
        &self.registry
    }

    /// This function will start tokio runtime and will finish once the
    /// `SystemExit` message get received.
    /// Function `f` get called within tokio runtime context.
    pub fn run<F>(f: F) -> i32
    where
        F: FnOnce() + Send + 'static,
    {
        let (_rt, stop) = System::create_runtime("actix-thread-pool-", 1, f);

        match stop.wait() {
            Ok(code) => code,
            Err(_) => 1,
        }
    }

    fn create_runtime<F>(name: &str, pool_size: usize, f: F) -> (Runtime, Receiver<i32>)
    where
        F: FnOnce() + Send + 'static,
    {
        let (stop_tx, stop) = channel();
        let (addr_sender, addr_receiver) = addr_channel::channel(16);

        let addr = Addr::new(addr_sender);
        let system = System {
            arbiter: addr.clone(),
            registry: SystemRegistry::new(addr),
        };
        System::set_current(system.clone());

        // system arbiter
        let arb = SystemArbiter {
            arbiters: HashMap::new(),
            stop: Some(stop_tx),
        };

        let mut threadpool = ThreadPoolBuilder::new();
        threadpool
            .name_prefix(name)
            .pool_size(pool_size)
            .after_start(move || {
                System::set_current(system.clone());
            });

        let mut rt = Builder::new()
            .threadpool_builder(threadpool)
            .build()
            .unwrap();

        // init system arbiter and run configuration method
        let (tx, rx) = channel();
        let _ = rt.spawn(future::lazy(move || {
            tokio::spawn(Context::with_receiver(Some(arb), addr_receiver));

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
pub struct SystemRunner {
    rt: Runtime,
    stop: Receiver<i32>,
}

impl SystemRunner {
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

pub struct SystemArbiter {
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
