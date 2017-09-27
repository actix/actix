use std;
use std::thread;
use std::cell::RefCell;
use uuid::Uuid;
use tokio_core::reactor::{Core, Remote, Handle};
use futures::future;
use futures::sync::oneshot::{channel, Sender, Receiver};

use actor::{Actor, MessageHandler};
use address::{Address, SyncAddress};
use builder::ActorBuilder;
use context::Context;
use message::{MessageFuture, MessageFutureResult};
use registry::{Registry, SystemRegistry};
use system::{System, RegisterArbiter, UnregisterArbiter};

thread_local!(
    static HND: RefCell<Option<Handle>> = RefCell::new(None);
    static STOP: RefCell<Option<Sender<i32>>> = RefCell::new(None);
    static ADDR: RefCell<Option<Address<Arbiter>>> = RefCell::new(None);
    static REG: RefCell<Option<Registry>> = RefCell::new(None);
    static SYS: RefCell<Option<SyncAddress<System>>> = RefCell::new(None);
    static SYSNAME: RefCell<Option<String>> = RefCell::new(None);
    static SYSREG: RefCell<Option<SystemRegistry>> = RefCell::new(None);
);

pub struct Arbiter {
    h: Remote,
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

    pub fn new(name: Option<String>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();

        let id = Uuid::new_v4();
        let id_cloned = id;
        let sys = Arbiter::system();
        let sys_name = Arbiter::system_name();
        let sys_registry = Arbiter::system_registry().clone();
        let name = if let Some(n) = name {
            format!("arbiter:{:?}:{:?}", id.hyphenated().to_string(), n)
        } else {
            format!("arbiter:{:?}", id.hyphenated().to_string())
        };

        let _ = thread::Builder::new().name(name).spawn(move|| {
            let mut core = Core::new().unwrap();

            let (stop_tx, stop_rx) = channel();
            HND.with(|cell| *cell.borrow_mut() = Some(core.handle()));
            STOP.with(|cell| *cell.borrow_mut() = Some(stop_tx));

            // system
            SYS.with(|cell| *cell.borrow_mut() = Some(sys));
            SYSNAME.with(|cell| *cell.borrow_mut() = Some(sys_name));
            SYSREG.with(|cell| *cell.borrow_mut() = Some(sys_registry));

            // start arbiter
            let addr = ActorBuilder::start(
                Arbiter {h: core.remote(), sys: true, id: id});
            ADDR.with(|cell| *cell.borrow_mut() = Some(addr));

            if tx.send(core.remote()).is_err() {
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
        let remote = rx.recv().unwrap();

        Arbiter {h: remote, sys: false, id: id_cloned}
    }

    pub(crate) fn new_system() -> Core {
        let core = Core::new().unwrap();
        HND.with(|cell| *cell.borrow_mut() = Some(core.handle()));
        REG.with(|cell| *cell.borrow_mut() = Some(Registry::new()));
        SYSREG.with(|cell| *cell.borrow_mut() = Some(SystemRegistry::new()));

        // start arbiter
        let addr = ActorBuilder::start(
            Arbiter {h: core.remote(), sys: true, id: Uuid::new_v4()});
        ADDR.with(|cell| *cell.borrow_mut() = Some(addr));

        core
    }

    pub(crate) fn set_system(addr: SyncAddress<System>, name: String) {
        SYS.with(|cell| *cell.borrow_mut() = Some(addr));
        SYSNAME.with(|cell| *cell.borrow_mut() = Some(name));
    }

    /// Return current arbiter address
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

    pub fn start<F, T>(&self, f: F) -> Receiver<SyncAddress<T>>
        where T: Actor,
              F: 'static + Send + FnOnce(&mut Context<T>) -> T
    {
        let (tx, rx) = channel();
        self.h.spawn(move |_| {
            let addr = T::create(f);
            let _ = tx.send(addr);
            future::result(Ok(()))
        });

        rx
    }
}

impl Clone for Arbiter {
    fn clone(&self) -> Self {
        Arbiter {h: self.h.clone(), sys: false, id: self.id}
    }
}

impl Default for Arbiter {
    fn default() -> Self {
        Arbiter::new(None)
    }
}

impl MessageHandler<StopArbiter> for Arbiter {

    type Item = ();
    type Error = ();
    type InputError = ();

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
