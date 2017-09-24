use std;
use std::cell::RefCell;
use std::sync::Mutex;
use tokio_core::reactor::{Core, Remote, Handle};
use futures::unsync::oneshot::{channel, Sender, Receiver};

use fut;
use address::Address;
use builder::ServiceBuilder;
use context::Context;
use service::{Message, MessageFuture, MessageHandler, DefaultMessage, Service};

thread_local!(
    static H: RefCell<Option<Handle>> = RefCell::new(None);
    static ADDR: RefCell<Option<Address<System>>> = RefCell::new(None);
);
lazy_static! {
    static ref REMOTE: Mutex<RefCell<Option<Remote>>> = {
        Mutex::new(RefCell::new(None))
    };
}

pub fn get_system() -> Address<System> {
    ADDR.with(|cell| match *cell.borrow() {
        Some(ref addr) => addr.clone(),
        None => panic!("System is not running"),
    })
}

pub fn get_handle() -> &'static Handle {
    H.with(|cell| match *cell.borrow() {
        Some(ref h) => unsafe{std::mem::transmute(h)},
        None => panic!("System is not running"),
    })
}

pub fn init_system() -> SystemConfigurator {
    let core = Core::new().unwrap();
    let (stop_tx, stop_rx) = channel();

    H.with(|cell| {
        *cell.borrow_mut() = Some(core.handle());
    });

    // start system
    let sys = System {stop: Some(stop_tx)}.start();
    ADDR.with(|cell| {
        *cell.borrow_mut() = Some(sys.clone());
    });

    // set remote
    {
        let r = core.remote();
        let remote = REMOTE.lock().unwrap();
        *remote.borrow_mut() = Some(r);
    }

    SystemConfigurator {
        h: core.handle(),
        addr: sys,
        core: core,
        stop: stop_rx
    }
}

pub struct System {
    stop: Option<Sender<i32>>,
}

pub struct SystemConfigurator {
    h: Handle,
    addr: Address<System>,
    core: Core,
    stop: Receiver<i32>,
}

impl SystemConfigurator {

    pub fn handle(&self) -> &Handle {
        &self.h
    }

    pub fn get_address(&self) -> Address<System> {
        self.addr.clone()
    }

    pub fn run(self) {
        let SystemConfigurator { mut core, stop, ..} = self;

        // run loop
        let code = match core.run(stop) {
            Ok(code) => code,
            Err(_) => 1,
        };
        std::process::exit(code);
    }
}

impl Service for System {
    type Message = DefaultMessage;
}

pub struct SystemExit(pub i32);

impl Message for SystemExit {
    type Item = ();
    type Error = ();
}

impl MessageHandler<SystemExit> for System {

    fn handle(&mut self, msg: SystemExit,
              _: &mut Context<Self>) -> MessageFuture<SystemExit, Self>
    {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(msg.0);
        }
        Box::new(fut::ok(()))
    }
}
