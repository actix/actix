use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::default::Default;
use std::sync::{Arc, Mutex};

use actor::{Actor, Supervised};
use arbiter::Arbiter;
use address::{Address, SyncAddress};
use context::Context;
use supervisor::Supervisor;

/// Actors registry
///
/// Actor can register itself as a service. Service can be defined as
/// `ArbiterService` which is unique per arbiter or `SystemService` which is
/// unique per system.
///
/// # Example
///
/// ```rust
/// extern crate actix;
/// use actix::prelude::*;
///
/// struct Ping;
///
/// #[derive(Default)]
/// struct MyActor1;
///
/// impl Actor for MyActor1 {
///     type Context = Context<Self>;
/// }
/// impl Supervised for MyActor1 {}
///
/// impl ArbiterService for MyActor1 {
///    fn service_started(&mut self, ctx: &mut Context<Self>) {
///       println!("Service started");
///    }
/// }
///
/// impl ResponseType<Ping> for MyActor1 {
///    type Item = ();
///    type Error = ();
/// }
///
/// impl Handler<Ping> for MyActor1 {
///
///    fn handle(&mut self, _: Ping, ctx: &mut Context<Self>) -> Response<Self, Ping> {
///       println!("ping");
///       Arbiter::system().send(msgs::SystemExit(0));
///       Response::Reply(())
///    }
/// }
///
/// struct MyActor2;
///
/// impl Actor for MyActor2 {
///    type Context = Context<Self>;
///
///    fn started(&mut self, _: &mut Context<Self>) {
///       let act = Arbiter::registry().get::<MyActor1>();
///       act.send(Ping)
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

/// Trait defines arbiter's service.
#[allow(unused_variables)]
pub trait ArbiterService: Actor<Context=Context<Self>> + Supervised + Default {
    /// Method is called during service initialization.
    fn service_started(&mut self, ctx: &mut Context<Self>) {}
}

/// Trait defines system's service.
#[allow(unused_variables)]
pub trait SystemService: Actor<Context=Context<Self>> + Supervised + Default {
    /// Method is called during service initialization.
    fn service_started(&mut self, ctx: &mut Context<Self>) {}
}

impl Registry {

    pub(crate) fn new() -> Self {
        Registry{registry: RefCell::new(HashMap::new())}
    }

    /// Query registry for specific actor. Returns address of the actor.
    /// If actor is not registered, starts new actor and
    /// return address of newly created actor.
    pub fn get<A: ArbiterService + Actor<Context=Context<A>>>(&self) -> Address<A> {
        let id = TypeId::of::<A>();
        if let Some(addr) = self.registry.borrow().get(&id) {
            if let Some(addr) = addr.downcast_ref::<Address<A>>() {
                return addr.clone()
            }
        }
        let addr: Address<_> = A::create(|ctx| {
            let mut act = A::default();
            act.service_started(ctx);
            act
        });

        self.registry.borrow_mut().insert(id, Box::new(addr.clone()));
        addr
    }
}

/// System wide actors registry
///
/// System registry serves same purpose as [Registry](struct.Registry.html), except
/// it is shared across all arbiters.
pub struct SystemRegistry {
    #[cfg_attr(feature="cargo-clippy", allow(type_complexity))]
    registry: Arc<Mutex<RefCell<HashMap<TypeId, Box<Any>>>>>,
}

unsafe impl Send for SystemRegistry {}

impl SystemRegistry {
    pub(crate) fn new() -> Self {
        SystemRegistry{registry: Arc::new(Mutex::new(RefCell::new(HashMap::new())))}
    }

    /// Return addres of the service. If service actor is not running
    /// it get started in system arbiter.
    pub fn get<A: SystemService + Actor<Context=Context<A>>>(&self) -> SyncAddress<A> {
        if let Ok(hm) = self.registry.lock() {
            if let Some(addr) = hm.borrow().get(&TypeId::of::<A>()) {
                match addr.downcast_ref::<SyncAddress<A>>() {
                    Some(addr) =>
                        return addr.clone(),
                    None =>
                        error!("Got unknown value: {:?}", addr),
                }
            } else {
                let addr = Supervisor::start_in(Arbiter::system_arbiter(), false, |ctx| {
                    let mut act = A::default();
                    act.service_started(ctx);
                    act
                });
                return addr.expect("System is dead");
            }
        }
        panic!("System registry lock is poisoned");
    }
}

impl Clone for SystemRegistry {
    fn clone(&self) -> Self {
        SystemRegistry{registry: Arc::clone(&self.registry)}
    }
}
