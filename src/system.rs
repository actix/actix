use std;
use std::cell::RefCell;
use tokio_core::reactor::{Core, Handle};
use futures::sync::oneshot::{channel, Receiver, Sender};

use address::SyncAddress;
use arbiter::Arbiter;
use builder::ServiceBuilder;
use context::Context;
use message::{MessageFuture, MessageFutureResult};
use service::{Message, MessageHandler, DefaultMessage, Service};

thread_local!(
    static ADDR: RefCell<Option<SyncAddress<System>>> = RefCell::new(None);
);

#[must_use="System must be run"]
pub struct System {
    core: Option<Core>,
    stop: Option<Receiver<i32>>,
    tx: Option<Sender<i32>>,
}

impl System {

    /// This function returns system address.
    /// `System::init` has to be called before, otherwise `get` function panics.
    pub fn get() -> SyncAddress<System> {
        ADDR.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("System is not running"),
        })
    }

    /// Initialize system
    pub fn init() -> System {
        let (stop_tx, stop_rx) = channel();
        let core = Arbiter::new_system();

        // start system
        let sys = System {core: None, tx: Some(stop_tx), stop: None}.sync_start();
        ADDR.with(|cell| {
            *cell.borrow_mut() = Some(sys);
        });

        System {
            core: Some(core),
            stop: Some(stop_rx),
            tx: None,
        }
    }

    /// Returns handle to current event loop. Same as `Arbiter::handle()`
    pub fn handle(&self) -> &Handle {
        Arbiter::handle()
    }

    /// This function will start tokio event loop and will finish once the `SystemExit`
    /// get received. Once `SystemExit` message get received, process exits with code
    /// encoded in message.
    pub fn run(self) {
        let System { core, stop, ..} = self;

        // run loop
        let code = match core.unwrap().run(stop.unwrap()) {
            Ok(code) => code,
            Err(_) => 1,
        };
        std::process::exit(code);
    }
}

impl Service for System {
    type Message = DefaultMessage;
}

/// Stop system execution and exit process with encoded code.
pub struct SystemExit(pub i32);

impl Message for SystemExit {
    type Item = ();
    type Error = ();
}

impl MessageHandler<SystemExit> for System {

    fn handle(&mut self, msg: SystemExit, _: &mut Context<Self>)
              -> MessageFuture<SystemExit, Self>
    {
        if let Some(stop) = self.tx.take() {
            let _ = stop.send(msg.0);
        }
        ().to_result()
    }
}
