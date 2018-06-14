use futures::sync::oneshot::{channel, Sender};
use std;
use std::cell::RefCell;
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
use registry::Registry;
use system::{RegisterArbiter, System, UnregisterArbiter};

thread_local!(
    static ADDR: RefCell<Option<Addr<Arbiter>>> = RefCell::new(None);
    static NAME: RefCell<Option<String>> = RefCell::new(None);
    static REG: RefCell<Option<Registry>> = RefCell::new(None);
);

/// Event loop controller
///
/// Arbiter controls event loop in it's thread. Each arbiter runs in separate
/// thread. Arbiter provides several api for event loop access. Each arbiter
/// can belongs to specific `System` actor.
pub struct Arbiter {
    id: Uuid,
    stop: Option<Sender<i32>>,
}

impl Actor for Arbiter {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        // register arbiter within system
        System::current()
            .arbiter()
            .do_send(RegisterArbiter(self.id.simple().to_string(), ctx.address()));
    }
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

            System::set_current(sys);

            // start arbiter
            let addr =
                rt.block_on(future::lazy(move || {
                    let addr = Actor::start(Arbiter {
                        id,
                        stop: Some(stop),
                    });
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
            System::current()
                .arbiter()
                .do_send(UnregisterArbiter(id.simple().to_string()));
        });

        rx.recv().unwrap()
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
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(msg.0);
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

impl<I: Send, E: Send> Handler<Execute<I, E>> for Arbiter {
    type Result = Result<I, E>;

    fn handle(&mut self, msg: Execute<I, E>, _: &mut Context<Self>) -> Result<I, E> {
        msg.exec()
    }
}
