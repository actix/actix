use futures::sync::oneshot::{channel, Sender};
use std;
use std::cell::{Cell, RefCell};
use std::thread;

use futures::{future, Future, IntoFuture};
use tokio::executor::current_thread::TaskExecutor;
use tokio::runtime::current_thread::Runtime;
use uuid::Uuid;

use actor::{Actor, AsyncContext};
use address::{channel, Addr};
use context::Context;
use handler::Handler;
use mailbox::DEFAULT_CAPACITY;
use msgs::{Execute, StartActor, StopArbiter};
use registry::SystemRegistry;
use system::{RegisterArbiter, System, UnregisterArbiter};

thread_local!(
    static RUNNING: Cell<bool> = Cell::new(false);
    static STOP: RefCell<Option<Sender<i32>>> = RefCell::new(None);
    static ADDR: RefCell<Option<Addr<Arbiter>>> = RefCell::new(None);
    static REG: RefCell<Option<SystemRegistry>> = RefCell::new(None);
    static NAME: RefCell<Option<String>> = RefCell::new(None);
    static SYS: RefCell<Option<Addr<System>>> = RefCell::new(None);
    static SYSARB: RefCell<Option<Addr<Arbiter>>> = RefCell::new(None);
    static SYSNAME: RefCell<Option<String>> = RefCell::new(None);
    static Q: RefCell<Vec<Box<Future<Item=(), Error=()>>>> = RefCell::new(Vec::new());
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
        let sys = Arbiter::system();
        let sys_name = Arbiter::system_name();
        let sys_arbiter = Arbiter::system_arbiter();
        let name = format!("arbiter:{}:{}", id.hyphenated().to_string(), name.into());

        let _ = thread::Builder::new().name(name.clone()).spawn(move || {
            let mut rt = Runtime::new().unwrap();

            let (stop_tx, stop_rx) = channel();
            STOP.with(|cell| *cell.borrow_mut() = Some(stop_tx));
            NAME.with(|cell| *cell.borrow_mut() = Some(name));
            //REG.with(|cell| *cell.borrow_mut() = Some(Registry::new()));
            RUNNING.with(|cell| cell.set(true));

            // system
            SYS.with(|cell| *cell.borrow_mut() = Some(sys));
            SYSARB.with(|cell| *cell.borrow_mut() = Some(sys_arbiter));
            SYSNAME.with(|cell| *cell.borrow_mut() = Some(sys_name));

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

    pub(crate) fn new_system(addr: Addr<System>, name: String) {
        REG.with(|cell| *cell.borrow_mut() = Some(SystemRegistry::new()));
        NAME.with(|cell| *cell.borrow_mut() = Some(name.clone()));
        SYS.with(|cell| *cell.borrow_mut() = Some(addr));
        SYSNAME.with(|cell| *cell.borrow_mut() = Some(name));

        // start arbiter
        let addr = Actor::start(Arbiter {
            sys: true,
            id: Uuid::new_v4(),
        });
        ADDR.with(|cell| *cell.borrow_mut() = Some(addr.clone()));
        SYSARB.with(|cell| *cell.borrow_mut() = Some(addr));
    }

    pub(crate) fn run_system() {
        RUNNING.with(|cell| cell.set(true));
        Q.with(|cell| {
            let mut exec = TaskExecutor::current();
            let mut v = cell.borrow_mut();
            for fut in v.drain(..) {
                exec.spawn_local(fut).unwrap();
            }
        });
    }

    #[inline]
    pub(crate) fn stop_system() {
        RUNNING.with(|cell| cell.set(false));
    }

    /// Returns current arbiter's address
    pub fn name() -> String {
        NAME.with(|cell| match *cell.borrow() {
            Some(ref name) => name.clone(),
            None => "Arbiter is not running".into(),
        })
    }

    /// Returns current arbiter's address
    pub fn arbiter() -> Addr<Arbiter> {
        ADDR.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("Arbiter is not running"),
        })
    }

    /// This function returns system address,
    pub fn system() -> Addr<System> {
        SYS.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("System is not running"),
        })
    }

    /// This function returns system address,
    pub fn system_arbiter() -> Addr<Arbiter> {
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

    /// Executes a future on the current thread.
    pub fn spawn<F>(future: F)
    where
        F: Future<Item = (), Error = ()> + 'static,
    {
        RUNNING.with(move |cell| {
            if cell.get() {
                TaskExecutor::current()
                    .spawn_local(Box::new(future))
                    .unwrap();
            } else {
                Q.with(move |cell| cell.borrow_mut().push(Box::new(future)));
            }
        });
    }

    /// Executes a future on the current thread.
    pub fn spawn_fn<F, R>(f: F)
    where
        F: FnOnce() -> R + 'static,
        R: IntoFuture<Item = (), Error = ()> + 'static,
    {
        Arbiter::spawn(future::lazy(f))
    }

    /// This function returns arbiter's registry
    pub fn registry() -> &'static SystemRegistry {
        REG.with(|cell| match *cell.borrow() {
            Some(ref reg) => unsafe { &*(reg as *const _) },
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
            ctx.run();
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
    A: Actor<Context = Context<A>>,
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
