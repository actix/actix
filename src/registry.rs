use std::any::{Any, TypeId};
use std::sync::{Arc, Mutex};
use std::cell::RefCell;
use std::collections::HashMap;

use actor::Actor;
use builder::ActorBuilder;
use address::{Address, SyncAddress};

/// Per type actors registry
///
/// Actor can register itself in registry with `register` methd, address
/// of actor can be queried with `query` method. Only one actor can be registered.
/// Registry access is relativly expensive, it should not be used on hot path.
///
/// # Example
///
/// ```rust
/// extern crate actix;
/// use actix::prelude::*;
///
/// struct Ping;
///
/// struct MyActor1;
///
/// impl Actor for MyActor1 {
///    fn started(&mut self, ctx: &mut Context<Self>) {
///       if let Err(_) = Arbiter::registry().register(ctx.address()) {
///           println!("Another actor is registered already");
///           ctx.stop();
///       }
///    }
/// }
///
/// impl MessageHandler<Ping> for MyActor1 {
///    type Item = ();
///    type Error = ();
///    type InputError = ();
///
///    fn handle(&mut self, _: Ping, ctx: &mut Context<Self>) -> MessageFuture<Self, Ping> {
///       println!("ping");
///       Arbiter::system().send(actix::SystemExit(0));
///       ().to_result()
///    }
///
/// }
///
/// struct MyActor2;
///
/// impl Actor for MyActor2 {
///    fn started(&mut self, _: &mut Context<Self>) {
///       if let Some(act) = Arbiter::registry().query::<MyActor1>() {
///           act.send(Ping)
///       }
///    }
/// }
///
///
/// fn main() {
///    // initialize system
///    let sys = System::new("test".to_owned());
///
///    // Start MyActor1
///    let _:() = MyActor1.start();
///
///    // Start MyActor2
///    let _:() = MyActor2.start();
///
///    // Run system, this function blocks current thread
///    let code = sys.run();
///    std::process::exit(code);
/// }
/// ```
pub struct Registry {
    registry: RefCell<HashMap<TypeId, Box<Any>>>,
}

impl Registry {

    pub(crate) fn new() -> Self {
        Registry{registry: RefCell::new(HashMap::new())}
    }

    /// Query registry for specific actor. On success returns address of the actor.
    /// If actor is not registered, starts new actor and
    /// return address of newly created actor.
    pub fn get<A: Actor + Default>(&self) -> Address<A> {
        let id = TypeId::of::<A>();
        if let Some(addr) = self.registry.borrow().get(&id) {
            if let Some(addr) = addr.downcast_ref::<Address<A>>() {
                return addr.clone()
            }
        }
        let addr: Address<_> = A::run();
        self.registry.borrow_mut().insert(id, Box::new(addr.clone()));
        addr
    }

    /// Query registry for specific actor. On success returns address of the actor.
    pub fn query<A: Actor>(&self) -> Option<Address<A>> {
        if let Some(addr) = self.registry.borrow().get(&TypeId::of::<A>()) {
            match addr.downcast_ref::<Address<A>>() {
                Some(addr) =>
                    return Some(addr.clone()),
                None =>
                    error!("Got unknown value: {:?}", addr),
            }
        }
        None
    }

    /// Register actor's address. If actor with same type already registered, returns `Err`
    pub fn register<A: Actor>(&self, addr: Address<A>) -> Result<(), Address<A>> {
        let id = TypeId::of::<A>();
        if self.registry.borrow().get(&id).is_some() {
            return Err(addr)
        }
        self.registry.borrow_mut().insert(id, Box::new(addr));
        Ok(())
    }
}

/// System wide actors registry
///
/// System registry surves same purpose as [Registry](struct.SystemRegistry.html), except
/// it is shared across all arbiters which runs withing same `System`.
pub struct SystemRegistry {
    #[cfg_attr(feature="cargo-clippy", allow(type_complexity))]
    registry: Arc<Mutex<RefCell<HashMap<TypeId, Box<Any>>>>>,
}

unsafe impl Send for SystemRegistry {}

impl SystemRegistry {
    pub(crate) fn new() -> Self {
        SystemRegistry{registry: Arc::new(Mutex::new(RefCell::new(HashMap::new())))}
    }

    /// Query registry for the address of specific actor.
    pub fn query<A: Actor>(&self) -> Option<SyncAddress<A>> {
        if let Ok(hm) = self.registry.lock() {
            if let Some(addr) = hm.borrow().get(&TypeId::of::<A>()) {
                match addr.downcast_ref::<SyncAddress<A>>() {
                    Some(addr) =>
                        return Some(addr.clone()),
                    None =>
                        error!("Got unknown value: {:?}", addr),
                }
            }
        }
        None
    }

    /// Register actor's address. If actor with same type already registered, returns `Err`
    pub fn register<A: Actor>(&self, addr: SyncAddress<A>) -> Result<(), SyncAddress<A>> {
        if let Ok(hm) = self.registry.lock() {
            let id = TypeId::of::<A>();
            if hm.borrow().get(&id).is_some() {
                return Err(addr)
            }
            hm.borrow_mut().insert(id, Box::new(addr));
            Ok(())
        } else {
            panic!("Mutex is poisoned");
        }
    }
}

impl Clone for SystemRegistry {
    fn clone(&self) -> Self {
        SystemRegistry{registry: Arc::clone(&self.registry)}
    }
}
