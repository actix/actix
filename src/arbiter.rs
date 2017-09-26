use std;
use std::thread;
use std::cell::RefCell;
use rand::{self, Rng};
use tokio_core::reactor::{Core, Remote, Handle};
use futures::future;
use futures::sync::oneshot::{channel, Sender, Receiver};

use actor::{Actor, MessageHandler};
use address::{Address, SyncAddress};
use builder::ActorBuilder;
use context::Context;
use message::{MessageFuture, MessageFutureResult};

thread_local!(
    static HND: RefCell<Option<Handle>> = RefCell::new(None);
    static STOP: RefCell<Option<Sender<i32>>> = RefCell::new(None);
    static ADDR: RefCell<Option<Address<Arbiter>>> = RefCell::new(None);
);

pub struct Arbiter {
    h: Remote,
    sys: bool,
}

impl Actor for Arbiter {}

impl Arbiter {

    pub fn new(name: Option<String>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();

        let name = if let Some(n) = name {
            format!("user:{:?}", n)
        } else {
            format!("user:{:?}", rand::thread_rng().gen::<u64>())
        };

        let _ = thread::Builder::new().name(name).spawn(move|| {
            let mut core = Core::new().unwrap();

            let (stop_tx, stop_rx) = channel();
            HND.with(|cell| *cell.borrow_mut() = Some(core.handle()));
            STOP.with(|cell| *cell.borrow_mut() = Some(stop_tx));

            // start arbiter
            let addr = ActorBuilder::start(Arbiter {h: core.remote(), sys: true});
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
        });
        let remote = rx.recv().unwrap();

        Arbiter {h: remote, sys: false}
    }

    pub(crate) fn new_system() -> Core {
        let core = Core::new().unwrap();
        HND.with(|cell| *cell.borrow_mut() = Some(core.handle()));

        // start arbiter
        let addr = ActorBuilder::start(Arbiter {h: core.remote(), sys: true});
        ADDR.with(|cell| *cell.borrow_mut() = Some(addr));

        core
    }

    /// Return current arbiter address
    pub fn get() -> Address<Arbiter> {
        ADDR.with(|cell| match *cell.borrow() {
            Some(ref addr) => addr.clone(),
            None => panic!("Arbiter is not running"),
        })
    }

    pub fn handle() -> &'static Handle {
        HND.with(|cell| match *cell.borrow() {
            Some(ref h) => unsafe{std::mem::transmute(h)},
            None => panic!("Arbiter is not running"),
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
        Arbiter {h: self.h.clone(), sys: false}
    }
}

impl Default for Arbiter {
    fn default() -> Self {
        Arbiter::new(None)
    }
}

/// Stop arbiter execution.
pub struct StopArbiter(pub i32);

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

/// Request `SyncAddress<Arbiter>` of the arbiter
pub struct ArbiterAddress;

impl MessageHandler<ArbiterAddress> for Arbiter {
    type Item = SyncAddress<Arbiter>;
    type Error = ();
    type InputError = ();

    fn handle(&mut self, _: ArbiterAddress, ctx: &mut Context<Self>)
              -> MessageFuture<Self, ArbiterAddress>
    {
        ctx.sync_address().to_result()
    }
}
