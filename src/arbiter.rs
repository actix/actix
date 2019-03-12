use std;
use std::cell::{Cell, RefCell};
use std::thread;

use futures::sync::oneshot::{channel, Sender};
use futures::{future, Future, IntoFuture};
use tokio::executor::current_thread::spawn;
use tokio::runtime::current_thread::Builder as RuntimeBuilder;
use uuid::Uuid;

use actor::Actor;
use address::{channel, Addr, AddressReceiver};
use clock::Clock;
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

/// An event loop controller.
///
/// An arbiter controls the event loop in its thread. Each arbiter
/// runs in a separate thread and provides several methods for event
/// loop access. Each arbiter can belong to a specific `System` actor.
///
/// By default, a panic in an arbiter does _not_ stop the rest of the
/// system, unless the panic is in the system actor. Users of an
/// arbiter can opt into shutting down the system on panic by using
/// `Arbiter::builder()` and enabling `stop_system_on_panic`.
#[derive(Debug)]
pub struct Arbiter {
    stop: Option<Sender<i32>>,
    stop_system_on_panic: bool,
}

impl Actor for Arbiter {
    type Context = Context<Self>;
}

impl Drop for Arbiter {
    fn drop(&mut self) {
        if self.stop_system_on_panic && thread::panicking() {
            eprintln!("Panic in Arbiter thread, shutting down system.");
            System::current().stop_with_code(1)
        }
    }
}

impl Arbiter {
    /// Returns a builder object for customized arbiter creation.
    pub fn builder() -> ArbiterBuilder {
        ArbiterBuilder::new()
    }

    /// Spawns a new thread and runs the event loop in the spawned thread.
    ///
    /// Returns the address of the newly created arbiter. Does not
    /// stop the system on panic.
    pub fn new<T: Into<String>>(name: T) -> Addr<Self> {
        Self::new_with_builder(Self::builder().name(name))
    }

    /// Spawns new thread and runs an event loop in that thread.
    ///
    /// Returns the address of the newly created arbiter.
    fn new_with_builder(mut builder: ArbiterBuilder) -> Addr<Self> {
        let (tx, rx) = std::sync::mpsc::channel();
        let id = Uuid::new_v4();
        let name = format!(
            "arbiter:{}:{}",
            id.to_hyphenated_ref().to_string(),
            builder.name.take().unwrap_or_else(|| "actor".into())
        );
        let sys = System::current();

        let _ = thread::Builder::new().name(name.clone()).spawn(move || {
            let mut rt = builder.runtime.build().unwrap();

            let (stop, stop_rx) = channel();
            NAME.with(|cell| *cell.borrow_mut() = Some(name));
            REG.with(|cell| *cell.borrow_mut() = Some(Registry::new()));
            RUNNING.with(|cell| cell.set(true));

            System::set_current(sys);

            // start arbiter
            let addr = rt
                .block_on(future::lazy(move || {
                    let addr = Actor::start(Self {
                        stop: Some(stop),
                        stop_system_on_panic: builder.stop_system_on_panic,
                    });
                    Ok::<_, ()>(addr)
                })).unwrap();
            ADDR.with(|cell| *cell.borrow_mut() = Some(addr.clone()));

            // register arbiter
            System::current().sys().do_send(RegisterArbiter(
                id.to_simple_ref().to_string(),
                addr.clone(),
            ));

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
                .do_send(UnregisterArbiter(id.to_simple_ref().to_string()));
        });

        rx.recv().unwrap()
    }

    pub(crate) fn new_system(rx: AddressReceiver<Self>, name: String) {
        NAME.with(|cell| *cell.borrow_mut() = Some(name));
        REG.with(|cell| *cell.borrow_mut() = Some(Registry::new()));
        RUNNING.with(|cell| cell.set(false));

        // start arbiter
        let ctx = Context::with_receiver(rx);
        let fut = ctx.into_future(Self {
            stop: None,
            stop_system_on_panic: true, // If the system Arbiter panics, stop the system.
        });
        let addr = fut.address();
        Self::spawn(fut);
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

    /// Returns current arbiter's name
    pub fn name() -> String {
        NAME.with(|cell| match *cell.borrow() {
            Some(ref name) => name.clone(),
            None => "Arbiter is not running".into(),
        })
    }

    /// Returns the current arbiter's address.
    pub fn current() -> Addr<Self> {
        ADDR.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("Arbiter is not running"),
        })
    }

    /// Returns the arbiter's registry,
    pub fn registry() -> Registry {
        REG.with(|cell| match *cell.borrow() {
            Some(ref reg) => reg.clone(),
            None => panic!("System is not running: {}", Self::name()),
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

    /// Executes a lazily constructed future on the current thread.
    ///
    /// The provided closure is run as part of future execution. After
    /// it returns, execution will continue with the future created by
    /// the closure.
    pub fn spawn_fn<F, R>(f: F)
    where
        F: FnOnce() -> R + 'static,
        R: IntoFuture<Item = (), Error = ()> + 'static,
    {
        Self::spawn(future::lazy(f))
    }

    /// Starts an actor inside a newly created arbiter.
    ///
    /// Returns the address of the actor created.
    pub fn start<A, F>(f: F) -> Addr<A>
    where
        A: Actor<Context = Context<A>>,
        F: FnOnce(&mut A::Context) -> A + Send + 'static,
    {
        Self::builder().start(f)
    }

    fn start_with_builder<A, F>(builder: ArbiterBuilder, f: F) -> Addr<A>
    where
        A: Actor<Context = Context<A>>,
        F: FnOnce(&mut A::Context) -> A + Send + 'static,
    {
        let (stx, srx) = channel::channel(DEFAULT_CAPACITY);

        // new arbiter
        let addr = Self::new_with_builder(builder);

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

/// A builder to create a customized arbiter.
pub struct ArbiterBuilder {
    /// Name of the Arbiter. Defaults to "actor" if unset.
    name: Option<String>,

    /// Whether the Arbiter will stop the whole System on uncaught panic. Defaults to false.
    stop_system_on_panic: bool,

    /// Tokio runtime builder.
    runtime: RuntimeBuilder,
}

impl ArbiterBuilder {
    fn new() -> Self {
        Self {
            name: None,
            stop_system_on_panic: false,
            runtime: RuntimeBuilder::new(),
        }
    }

    /// Sets the name of the arbiter.
    pub fn name<T: Into<String>>(mut self, name: T) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the option `stop_system_on_panic` which controls whether
    /// the system is stopped when an uncaught panic is thrown from an
    /// actor in the arbiter.
    ///
    /// Defaults to `false`.
    pub fn stop_system_on_panic(mut self, stop_system_on_panic: bool) -> Self {
        self.stop_system_on_panic = stop_system_on_panic;
        self
    }

    /// Sets the clock instance that will be used by this arbiter.
    ///
    /// Defaults to the clock used by the actix `System`, which
    /// defaults to the system clock.
    pub fn clock(mut self, clock: Clock) -> Self {
        self.runtime.clock(clock);
        self
    }

    /// Spawns a new thread and runs the event loop in the spawned
    /// thread.
    ///
    /// Returns address of newly created arbiter.
    pub fn build(self) -> Addr<Arbiter> {
        Arbiter::new_with_builder(self)
    }

    /// Starts an actor inside a newly created arbiter.
    ///
    /// Returns the address of the actor created.
    pub fn start<A, F>(self, f: F) -> Addr<A>
    where
        A: Actor<Context = Context<A>>,
        F: FnOnce(&mut A::Context) -> A + Send + 'static,
    {
        Arbiter::start_with_builder(self, f)
    }
}
