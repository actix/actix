use std;
use std::thread;
use std::cell::RefCell;
use uuid::Uuid;
use tokio_core::reactor::{Core, Handle};
use futures::sync::oneshot::{channel, Sender};

use actor::{Actor, MessageHandler, MessageResponse};
use address::{Address, SyncAddress};
use builder::ActorBuilder;
use context::Context;
use message::{MessageFuture, MessageFutureResult, MessageFutureError};
use registry::{Registry, SystemRegistry};
use system::{System, RegisterArbiter, UnregisterArbiter};

thread_local!(
    static HND: RefCell<Option<Handle>> = RefCell::new(None);
    static STOP: RefCell<Option<Sender<i32>>> = RefCell::new(None);
    static ADDR: RefCell<Option<Address<Arbiter>>> = RefCell::new(None);
    static REG: RefCell<Option<Registry>> = RefCell::new(None);
    static NAME: RefCell<Option<String>> = RefCell::new(None);
    static SYS: RefCell<Option<SyncAddress<System>>> = RefCell::new(None);
    static SYSARB: RefCell<Option<SyncAddress<Arbiter>>> = RefCell::new(None);
    static SYSNAME: RefCell<Option<String>> = RefCell::new(None);
    static SYSREG: RefCell<Option<SystemRegistry>> = RefCell::new(None);
);

/// Arbiter
///
/// Arbiter controls event loop in it's thread. Each arbiter runs in separate
/// thread. Arbiter provides several api for event loop acces. Each arbiter
/// can belongs to specific `System` actor.
pub struct Arbiter {
    id: Uuid,
    sys: bool,
}

/// Stop arbiter execution.
pub struct StopArbiter(pub i32);


impl Actor for Arbiter {
    fn started(&mut self, ctx: &mut Context<Self>) {
        // register arbiter within system
        Arbiter::system().send(
            RegisterArbiter(self.id.simple().to_string(), ctx.address()));
    }
}

impl Arbiter {

    /// Spawn new thread and run event loop in spawned thread.
    /// Returns address of newly created arbiter.
    pub fn new(name: Option<String>) -> SyncAddress<Arbiter> {
        let (tx, rx) = std::sync::mpsc::channel();

        let id = Uuid::new_v4();
        let sys = Arbiter::system();
        let sys_name = Arbiter::system_name();
        let sys_arbiter = Arbiter::system_arbiter();
        let sys_registry = Arbiter::system_registry().clone();
        let name = if let Some(n) = name {
            format!("arbiter:{:?}:{:?}", id.hyphenated().to_string(), n)
        } else {
            format!("arbiter:{:?}", id.hyphenated().to_string())
        };

        let _ = thread::Builder::new().name(name.clone()).spawn(move|| {
            let mut core = Core::new().unwrap();

            let (stop_tx, stop_rx) = channel();
            HND.with(|cell| *cell.borrow_mut() = Some(core.handle()));
            STOP.with(|cell| *cell.borrow_mut() = Some(stop_tx));
            NAME.with(|cell| *cell.borrow_mut() = Some(name));

            // system
            SYS.with(|cell| *cell.borrow_mut() = Some(sys));
            SYSARB.with(|cell| *cell.borrow_mut() = Some(sys_arbiter));
            SYSNAME.with(|cell| *cell.borrow_mut() = Some(sys_name));
            SYSREG.with(|cell| *cell.borrow_mut() = Some(sys_registry));

            // start arbiter
            let (addr, saddr) = ActorBuilder::start(
                Arbiter {sys: false, id: id});
            ADDR.with(|cell| *cell.borrow_mut() = Some(addr));

            if tx.send(saddr).is_err() {
                error!("Can not start Arbiter, remote side is dead");
            } else {
                // run loop
                let _ = match core.run(stop_rx) {
                    Ok(code) => code,
                    Err(_) => 1,
                };
            }

            // unregister arbiter
            Arbiter::system().send(
                UnregisterArbiter(id.simple().to_string()));
        });

        rx.recv().unwrap()
    }

    pub(crate) fn new_system(name: String) -> Core {
        let core = Core::new().unwrap();
        HND.with(|cell| *cell.borrow_mut() = Some(core.handle()));
        REG.with(|cell| *cell.borrow_mut() = Some(Registry::new()));
        NAME.with(|cell| *cell.borrow_mut() = Some(name));
        SYSREG.with(|cell| *cell.borrow_mut() = Some(SystemRegistry::new()));

        // start arbiter
        let (addr, sys_addr) = ActorBuilder::start(
            Arbiter {sys: true, id: Uuid::new_v4()});
        ADDR.with(|cell| *cell.borrow_mut() = Some(addr));
        SYSARB.with(|cell| *cell.borrow_mut() = Some(sys_addr));

        core
    }

    pub(crate) fn set_system(addr: SyncAddress<System>, name: String) {
        SYS.with(|cell| *cell.borrow_mut() = Some(addr));
        SYSNAME.with(|cell| *cell.borrow_mut() = Some(name));
    }

    /// Returns current arbiter's address
    pub fn name() -> String {
        NAME.with(|cell| match *cell.borrow() {
            Some(ref name) => name.clone(),
            None => panic!("Arbiter is not running"),
        })
    }

    /// Returns current arbiter's address
    pub fn arbiter() -> Address<Arbiter> {
        ADDR.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("Arbiter is not running"),
        })
    }

    /// This function returns system address,
    pub fn system() -> SyncAddress<System> {
        SYS.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("System is not running"),
        })
    }

    /// This function returns system address,
    pub fn system_arbiter() -> SyncAddress<Arbiter> {
        SYSARB.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("System is not running"),
        })
    }

    /// This function returns system name,
    pub fn system_name() -> String {
        SYSNAME.with(|cell| match *cell.borrow() {
            Some(ref name) => name.clone(),
            None => panic!("System is not running"),
        })
    }

    /// This function returns system registry,
    pub fn system_registry() -> &'static SystemRegistry {
        SYSREG.with(|cell| match *cell.borrow() {
            Some(ref reg) => unsafe{std::mem::transmute(reg)},
            None => panic!("System is not running"),
        })
    }

    /// This function returns current event loop's handle,
    pub fn handle() -> &'static Handle {
        HND.with(|cell| match *cell.borrow() {
            Some(ref h) => unsafe{std::mem::transmute(h)},
            None => panic!("Arbiter is not running"),
        })
    }

    /// This function returns arbiter's registry,
    pub fn registry() -> &'static Registry {
        REG.with(|cell| match *cell.borrow() {
            Some(ref reg) => unsafe{std::mem::transmute(reg)},
            None => panic!("System is not running"),
        })
    }
}

#[doc(hidden)]
impl MessageResponse<StopArbiter> for Arbiter {
    type Item = ();
    type Error = ();
}

impl MessageHandler<StopArbiter> for Arbiter {

    fn handle(&mut self, msg: StopArbiter, _: &mut Context<Self>)
              -> MessageFuture<Self, StopArbiter>
    {
        if self.sys {
            warn!("System arbiter received `StopArbiter` message.
                  To shutdown system `SystemExit` message should be send to `Address<System>`");
        } else {
            STOP.with(|cell| {
                if let Some(stop) = cell.borrow_mut().take() {
                    let _ = stop.send(msg.0);
                }
            });
        }
        ().to_result()
    }
}


/// Start actor in arbiter's thread
pub struct StartActor<A: Actor>(Box<FnBox<A>>);

impl<A: Actor> StartActor<A>
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

impl<A> MessageResponse<StartActor<A>> for Arbiter where A: Actor {
    type Item = SyncAddress<A>;
    type Error = ();
}

impl<A> MessageHandler<StartActor<A>> for Arbiter where A: Actor {

    fn handle(&mut self, msg: StartActor<A>, _: &mut Context<Self>)
              -> MessageFuture<Self, StartActor<A>>
    {
        msg.call().to_result()
    }
}


/// Execute function in target thread.
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
///
///    fn started(&mut self, ctx: &mut Context<Self>) {
///        self.addr.send(actix::Execute::new(|| -> Result<(), ()> {
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

/// Execute message response
impl<I: Send, E: Send> MessageResponse<Execute<I, E>> for Arbiter {
    type Item = I;
    type Error = E;
}

/// Execute function in arbiter's thread
impl<I: Send, E: Send> MessageHandler<Execute<I, E>> for Arbiter {

    fn handle(&mut self, msg: Execute<I, E>, _: &mut Context<Self>)
              -> MessageFuture<Self, Execute<I, E>>
    {
        match msg.exec() {
            Ok(i) => i.to_result(),
            Err(e) => e.to_error(),
        }
    }
}
