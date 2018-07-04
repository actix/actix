use std;
use std::cell::{Cell, RefCell};
use std::thread;

use futures::sync::oneshot::{channel, Sender};
use futures::{future, Future, IntoFuture};
use tokio::executor::current_thread::spawn;
use tokio::runtime::current_thread::Runtime;
use uuid::Uuid;

use actor::Actor;
use address::{channel, Addr, AddressReceiver};
use context::Context;
use handler::Handler;
use mailbox::DEFAULT_CAPACITY;
use msgs::{Execute, StartActor, StopArbiter};
use registry::Registry;
use system::{RegisterArbiter, System, UnregisterArbiter};

thread_local!(
    static ADDR: RefCell<Option<Addr<Arbiter>>> = RefCell::new(None);
    static NAME: RefCell<Option<String>> = RefCell::new(None);
    static REG: RefCell<Option<Registry>> = RefCell::new(None);
    static RUNNING: Cell<bool> = Cell::new(false);
    static Q: RefCell<Vec<Box<Future<Item=(), Error=()>>>> = RefCell::new(Vec::new());
);

/// Event loop controller
///
/// Arbiter controls event loop in it's thread. Each arbiter runs in separate
/// thread. Arbiter provides several api for event loop access. Each arbiter
/// can belongs to specific `System` actor.
pub struct Arbiter {
    stop: Option<Sender<i32>>,
}

impl Actor for Arbiter {
    type Context = Context<Self>;
}

impl Arbiter {
    /// Spawn new thread and run event loop in spawned thread.
    /// Returns address of newly created arbiter.
    pub fn new<T: Into<String>>(name: T) -> Addr<Arbiter> {
        let (tx, rx) = std::sync::mpsc::channel();

        let id = Uuid::new_v4();
        let name = format!("arbiter:{}:{}", id.hyphenated().to_string(), name.into());
        let sys = System::current();

        let _ = thread::Builder::new().name(name.clone()).spawn(move || {
            let mut rt = Runtime::new().unwrap();

            let (stop, stop_rx) = channel();
            NAME.with(|cell| *cell.borrow_mut() = Some(name));
            REG.with(|cell| *cell.borrow_mut() = Some(Registry::new()));
            RUNNING.with(|cell| cell.set(true));

            System::set_current(sys);

            // start arbiter
            let addr =
                rt.block_on(future::lazy(move || {
                    let addr = Actor::start(Arbiter { stop: Some(stop) });
                    Ok::<_, ()>(addr)
                })).unwrap();
            ADDR.with(|cell| *cell.borrow_mut() = Some(addr.clone()));

            // register arbiter
            System::current()
                .sys()
                .do_send(RegisterArbiter(id.simple().to_string(), addr.clone()));

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
            System::current()
                .sys()
                .do_send(UnregisterArbiter(id.simple().to_string()));
        });

        rx.recv().unwrap()
    }

    pub(crate) fn new_system(rx: AddressReceiver<Arbiter>, name: String) {
        NAME.with(|cell| *cell.borrow_mut() = Some(name));
        REG.with(|cell| *cell.borrow_mut() = Some(Registry::new()));
        RUNNING.with(|cell| cell.set(false));

        // start arbiter
        let ctx = Context::with_receiver(rx);
        let fut = ctx.into_future(Arbiter { stop: None });
        let addr = fut.address();
        Arbiter::spawn(fut);
        ADDR.with(|cell| *cell.borrow_mut() = Some(addr.clone()));
    }

    pub(crate) fn run_system() {
        RUNNING.with(|cell| cell.set(true));
        Q.with(|cell| {
            let mut v = cell.borrow_mut();
            for fut in v.drain(..) {
                spawn(fut);
            }
        });
    }

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
    pub fn current() -> Addr<Arbiter> {
        ADDR.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("Arbiter is not running"),
        })
    }

    /// This function returns arbiter's registry,
    pub fn registry() -> Registry {
        REG.with(|cell| match *cell.borrow() {
            Some(ref reg) => reg.clone(),
            None => panic!("System is not running: {}", Arbiter::name()),
        })
    }

    /// Executes a future on the current thread.
    pub fn spawn<F>(future: F)
    where
        F: Future<Item = (), Error = ()> + 'static,
    {
        RUNNING.with(move |cell| {
            if cell.get() {
                spawn(Box::new(future));
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
            let mut ctx = Context::with_receiver(srx);
            let act = f(&mut ctx);
            let fut = ctx.into_future(act);
            spawn(fut);
            Ok(())
        }));

        Addr::new(stx)
    }
}

impl Handler<StopArbiter> for Arbiter {
    type Result = ();

    fn handle(&mut self, msg: StopArbiter, _: &mut Context<Self>) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(msg.0);
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

impl<I: Send, E: Send> Handler<Execute<I, E>> for Arbiter {
    type Result = Result<I, E>;

    fn handle(&mut self, msg: Execute<I, E>, _: &mut Context<Self>) -> Result<I, E> {
        msg.exec()
    }
}
