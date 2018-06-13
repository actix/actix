use futures::sync::oneshot::{channel, Sender};
use std;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::thread;

use futures::{future, Future};
use tokio::executor::current_thread::spawn;
use tokio::runtime::current_thread::Runtime;
use uuid::Uuid;

use actor::{Actor, AsyncContext};
use address::{channel, Addr};
use context::Context;
use handler::Handler;
use mailbox::DEFAULT_CAPACITY;
use msgs::{Execute, StartActor, StopArbiter};
use registry::{Registry, SystemRegistry};
use system::{RegisterArbiter, System, UnregisterArbiter};

thread_local!(
    static STOP: RefCell<Option<Sender<i32>>> = RefCell::new(None);
    static ADDR: RefCell<Option<Addr<Arbiter>>> = RefCell::new(None);
    static NAME: RefCell<Option<String>> = RefCell::new(None);
    static REG: RefCell<Option<Registry>> = RefCell::new(None);

    static SYS: RefCell<Arc<Mutex<Option<Addr<System>>>>> =
        RefCell::new(Arc::new(Mutex::new(None)));
    static SYSARB: RefCell<Arc<Mutex<Option<Addr<Arbiter>>>>> =
        RefCell::new(Arc::new(Mutex::new(None)));
    static SYSREG: RefCell<Option<SystemRegistry>> = RefCell::new(None);
);

/// Event loop controller
///
/// Arbiter controls event loop in it's thread. Each arbiter runs in separate
/// thread. Arbiter provides several api for event loop access. Each arbiter
/// can belongs to specific `System` actor.
pub struct Arbiter {
    id: Uuid,
    sys: bool,
}

impl Actor for Arbiter {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        // register arbiter within system
        if !self.sys {
            Arbiter::system()
                .do_send(RegisterArbiter(self.id.simple().to_string(), ctx.address()));
        }
    }
}

impl Arbiter {
    /// Spawn new thread and run event loop in spawned thread.
    /// Returns address of newly created arbiter.
    pub fn new<T: Into<String>>(name: T) -> Addr<Arbiter> {
        let (tx, rx) = std::sync::mpsc::channel();

        let id = Uuid::new_v4();
        let name = format!("arbiter:{}:{}", id.hyphenated().to_string(), name.into());
        let sys = Arbiter::system_ref();
        let sysreg = Arbiter::system_reg();
        let sys_arbiter = Arbiter::system_arb_ref();

        let _ = thread::Builder::new().name(name.clone()).spawn(move || {
            let mut rt = Runtime::new().unwrap();

            let (stop_tx, stop_rx) = channel();
            STOP.with(|cell| *cell.borrow_mut() = Some(stop_tx));
            NAME.with(|cell| *cell.borrow_mut() = Some(name));
            REG.with(|cell| *cell.borrow_mut() = Some(Registry::new()));

            SYS.with(|cell| *cell.borrow_mut() = sys);
            SYSARB.with(|cell| *cell.borrow_mut() = sys_arbiter);
            SYSREG.with(|cell| *cell.borrow_mut() = Some(sysreg));

            // start arbiter
            let addr =
                rt.block_on(future::lazy(move || {
                    let addr = Actor::start(Arbiter { id, sys: false });
                    Ok::<_, ()>(addr)
                })).unwrap();
            ADDR.with(|cell| *cell.borrow_mut() = Some(addr.clone()));

            if tx.send(addr).is_err() {
                error!("Can not start Arbiter, remote side is dead");
            } else {
                // run loop
                let _ = match rt.block_on(stop_rx) {
                    Ok(code) => code,
                    Err(_) => 1,
                };
            }

            // unregister arbiter
            Arbiter::system().do_send(UnregisterArbiter(id.simple().to_string()));
        });

        rx.recv().unwrap()
    }

    pub(crate) fn system_ref() -> Arc<Mutex<Option<Addr<System>>>> {
        SYS.with(|s| s.borrow().clone())
    }

    pub(crate) fn system_arb_ref() -> Arc<Mutex<Option<Addr<Arbiter>>>> {
        SYSARB.with(|s| s.borrow().clone())
    }

    pub(crate) fn system_reg() -> SystemRegistry {
        SYSREG.with(|s| {
            if s.borrow().is_none() {
                *s.borrow_mut() = Some(SystemRegistry::new());
            }
            s.borrow().clone().unwrap()
        })
    }

    pub(crate) fn set_system_ref(addr: Arc<Mutex<Option<Addr<System>>>>) {
        SYS.with(move |cell| *cell.borrow_mut() = addr);
    }

    pub(crate) fn set_system_arb_ref(addr: Arc<Mutex<Option<Addr<Arbiter>>>>) {
        SYSARB.with(move |cell| *cell.borrow_mut() = addr);
    }

    pub fn set_system_reg(reg: SystemRegistry) {
        SYSREG.with(|cell| *cell.borrow_mut() = Some(reg));
    }

    pub(crate) fn new_system() -> Addr<Arbiter> {
        // start arbiter
        let addr = Actor::start(Arbiter {
            sys: true,
            id: Uuid::new_v4(),
        });
        Arbiter::system_reg().set_arbiter(addr.clone());
        addr
    }

    /// Returns current arbiter's address
    pub fn name() -> String {
        NAME.with(|cell| match *cell.borrow() {
            Some(ref name) => name.clone(),
            None => "Arbiter is not running".into(),
        })
    }

    /// Returns current arbiter's address
    pub fn current() -> Addr<Arbiter> {
        ADDR.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("Arbiter is not running"),
        })
    }

    /// This function returns system address,
    pub fn system() -> Addr<System> {
        SYS.with(|cell| {
            let b = cell.borrow();
            if let Some(a) = b.as_ref().lock().unwrap().clone() {
                return a;
            } else {
                panic!("System is not running");
            };
        })
    }

    /// This function returns system address,
    pub fn system_arbiter() -> Addr<Arbiter> {
        SYSARB.with(|cell| {
            let b = cell.borrow();
            if let Some(a) = b.as_ref().lock().unwrap().clone() {
                return a;
            } else {
                panic!("System is not running");
            };
        })
    }

    /// This function returns arbiter's registry
    pub fn system_registry() -> &'static SystemRegistry {
        SYSREG.with(|cell| match *cell.borrow() {
            Some(ref reg) => unsafe { &*(reg as *const _) },
            None => panic!("System is not running: {}", Arbiter::name()),
        })
    }

    /// This function returns arbiter's registry,
    pub fn registry() -> &'static Registry {
        REG.with(|cell| match *cell.borrow() {
            Some(ref reg) => unsafe { std::mem::transmute(reg) },
            None => panic!("System is not running: {}", Arbiter::name()),
        })
    }

    /// Start new arbiter and then start actor in created arbiter.
    /// Returns `Addr<Syn, A>` of created actor.
    pub fn start<A, F>(f: F) -> Addr<A>
    where
        A: Actor<Context = Context<A>>,
        F: FnOnce(&mut A::Context) -> A + Send + 'static,
    {
        let (stx, srx) = channel::channel(DEFAULT_CAPACITY);

        // new arbiter
        let addr = Arbiter::new("actor");

        // create actor
        addr.do_send::<Execute>(Execute::new(move || {
            let mut ctx = Context::with_receiver(None, srx);
            let act = f(&mut ctx);
            ctx.set_actor(act);
            spawn(ctx.map(|_| ()).map_err(|_| ()));
            Ok(())
        }));

        Addr::new(stx)
    }
}

impl Handler<StopArbiter> for Arbiter {
    type Result = ();

    fn handle(&mut self, msg: StopArbiter, _: &mut Context<Self>) {
        if self.sys {
            warn!(
                "System arbiter received `StopArbiter` message.
                  To shutdown system, `SystemExit` message should be
                  send to `Addr<System>`"
            );
        } else {
            STOP.with(|cell| {
                if let Some(stop) = cell.borrow_mut().take() {
                    let _ = stop.send(msg.0);
                }
            });
        }
    }
}

impl<A> Handler<StartActor<A>> for Arbiter
where
    A: Actor<Context = Context<A>> + Send,
{
    type Result = Addr<A>;

    fn handle(&mut self, msg: StartActor<A>, _: &mut Context<Self>) -> Addr<A> {
        msg.call()
    }
}

/// Execute function in arbiter's thread
impl<I: Send, E: Send> Handler<Execute<I, E>> for Arbiter {
    type Result = Result<I, E>;

    fn handle(&mut self, msg: Execute<I, E>, _: &mut Context<Self>) -> Result<I, E> {
        msg.exec()
    }
}
