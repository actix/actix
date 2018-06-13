//! Actors registry
//!
//! Actor can register itself as a service. Service can be defined as
//! `ArbiterService` which is unique per arbiter or `SystemService` which is
//! unique per system.
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::default::Default;
use std::sync::{Arc, Mutex};

use actor::{Actor, Supervised};
use address::Addr;
use arbiter::Arbiter;
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
/// # #[macro_use] extern crate actix;
/// use actix::prelude::*;
///
/// #[derive(Message)]
/// struct Ping;
///
/// #[derive(Default)]
/// struct MyActor1;
///
/// impl Actor for MyActor1 {
///     type Context = Context<Self>;
/// }
/// impl actix::Supervised for MyActor1 {}
///
/// impl ArbiterService for MyActor1 {
///    fn service_started(&mut self, ctx: &mut Context<Self>) {
///       println!("Service started");
///    }
/// }
///
/// impl Handler<Ping> for MyActor1 {
///    type Result = ();
///
///    fn handle(&mut self, _: Ping, ctx: &mut Context<Self>) {
///       println!("ping");
/// #     Arbiter::system().do_send(actix::msgs::SystemExit(0));
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
///       act.do_send(Ping);
///    }
/// }
///
///
/// fn main() {
///    // initialize system
///    let sys = System::new("test");
///
///    // Start MyActor1
///    let _:() = MyActor1.start();
///
///    // Start MyActor2
///    let _:() = MyActor2.start();
///
///    // Run system, this function blocks current thread
///    let code = sys.run();
/// #  std::process::exit(code);
/// }
/// ```
pub struct Registry {
    registry: RefCell<HashMap<TypeId, Box<Any>>>,
}

/// Trait defines arbiter's service.
#[allow(unused_variables)]
pub trait ArbiterService: Actor<Context = Context<Self>> + Supervised + Default {
    /// Method is called during service initialization.
    fn service_started(&mut self, ctx: &mut Context<Self>) {}

    /// Get actor's address from arbiter registry
    fn from_registry() -> Addr<Self> {
        Arbiter::registry().get::<Self>()
    }
}

impl Registry {
    pub(crate) fn new() -> Self {
        Registry {
            registry: RefCell::new(HashMap::new()),
        }
    }

    /// Query registry for specific actor. Returns address of the actor.
    /// If actor is not registered, starts new actor and
    /// return address of newly created actor.
    pub fn get<A: ArbiterService + Actor<Context = Context<A>>>(&self) -> Addr<A> {
        let id = TypeId::of::<A>();
        if let Some(addr) = self.registry.borrow().get(&id) {
            if let Some(addr) = addr.downcast_ref::<Addr<A>>() {
                return addr.clone();
            }
        }
        let addr: Addr<A> = Supervisor::start_in_current(|ctx| {
            let mut act = A::default();
            act.service_started(ctx);
            act
        });

        self.registry
            .borrow_mut()
            .insert(id, Box::new(addr.clone()));
        addr
    }

    /// Add new actor to the registry using the initialization function provided, panic if actor
    /// is already running
    pub fn set<A: ArbiterService + Actor<Context = Context<A>>>(&self, addr: Addr<A>) {
        let id = TypeId::of::<A>();
        if let Some(addr) = self.registry.borrow().get(&id) {
            match addr.downcast_ref::<Addr<A>>() {
                Some(_) => panic!("Actor already started"),
                None => {}
            }
        }

        self.registry
            .borrow_mut()
            .insert(id, Box::new(addr.clone()));
    }
}

/// Actors registry
///
/// Actor can register itself as a service. Service can be defined as
/// `ArbiterService` which is unique per arbiter or `SystemService` which is
/// unique per system.
///
/// # Example
///
/// ```rust
/// # #[macro_use] extern crate actix;
/// use actix::prelude::*;
///
/// #[derive(Message)]
/// struct Ping;
///
/// #[derive(Default)]
/// struct MyActor1;
///
/// impl Actor for MyActor1 {
///     type Context = Context<Self>;
/// }
/// impl actix::Supervised for MyActor1 {}
///
/// impl SystemService for MyActor1 {
///     fn service_started(&mut self, ctx: &mut Context<Self>) {
///         println!("Service started");
///     }
/// }
///
/// impl Handler<Ping> for MyActor1 {
///     type Result = ();
///
///     fn handle(&mut self, _: Ping, ctx: &mut Context<Self>) {
///         println!("ping");
/// #     Arbiter::system().do_send(actix::msgs::SystemExit(0));
///     }
/// }
///
/// struct MyActor2;
///
/// impl Actor for MyActor2 {
///     type Context = Context<Self>;
///
///     fn started(&mut self, _: &mut Context<Self>) {
///         let act = Arbiter::registry().get::<MyActor1>();
///         act.do_send(Ping);
///     }
/// }
///
/// fn main() {
///     // initialize system
///     let code = System::run(|| {
///         // Start MyActor1
///         let addr = MyActor1.start();
///
///         // Start MyActor2
///         let addr = MyActor2.start();
///     });
/// #   std::process::exit(code);
/// }
/// ```
/// System wide actors registry
///
/// System registry serves same purpose as [Registry](struct.Registry.html),
/// except it is shared across all arbiters.
pub struct SystemRegistry {
    arbiter: Arc<Mutex<Option<Addr<Arbiter>>>>,
    registry: Arc<Mutex<HashMap<TypeId, Box<Any + Send>>>>,
}

/// Trait defines system's service.
#[allow(unused_variables)]
pub trait SystemService:
    Actor<Context = Context<Self>> + Supervised + Send + Default
{
    /// Method is called during service initialization.
    fn service_started(&mut self, ctx: &mut Context<Self>) {}

    /// Get actor's address from system registry
    fn from_registry() -> Addr<Self> {
        Arbiter::system_reg().get::<Self>()
    }

    /*
    /// Create an SystemService with a closure
    fn init_actor<F>(f: F) -> Addr<Self>
    where
        F: FnOnce(&mut Self::Context) -> Self + Send + 'static,
    {
        Arbiter::registry().init_actor::<Self, F>(f)
    }*/
}

impl SystemRegistry {
    pub(crate) fn new() -> Self {
        SystemRegistry {
            arbiter: Arc::new(Mutex::new(None)),
            registry: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn set_arbiter(&mut self, addr: Addr<Arbiter>) {
        *self.arbiter.lock().unwrap() = Some(addr);
    }

    /// Return address of the service. If service actor is not running
    /// it get started in system arbiter.
    pub fn get<A: SystemService + Actor<Context = Context<A>> + Send>(&self) -> Addr<A> {
        {
            if let Ok(mut hm) = self.registry.lock() {
                if let Some(addr) = hm.get(&TypeId::of::<A>()) {
                    match addr.downcast_ref::<Addr<A>>() {
                        Some(addr) => return addr.clone(),
                        None => error!("Got unknown value: {:?}", addr),
                    }
                }
                let addr = if let Some(addr) = self.arbiter.lock().unwrap().as_ref() {
                    Supervisor::start_in(addr, |ctx| {
                        let mut act = A::default();
                        act.service_started(ctx);
                        act
                    })
                } else {
                    Supervisor::start(|ctx| {
                        let mut act = A::default();
                        act.service_started(ctx);
                        act
                    })
                };
                hm.insert(TypeId::of::<A>(), Box::new(addr.clone()));
                return addr;
            } else {
                panic!("System registry lock is poisoned");
            }
        }
    }

    /*
    /// Initialize a SystemService, panic if already started
    pub fn init_actor<A: SystemService + Actor<Context = Context<A>>, F>(
        &self, with: F,
    ) -> Addr<A>
    where
        F: FnOnce(&mut A::Context) -> A + Send + 'static,
    {
        let addr = Supervisor::start_in(&Arbiter::system_arbiter(), |ctx| {
            let mut act = with(ctx);
            act.service_started(ctx);
            act
        });

        self.set(addr.clone());
        addr
    }

    /// Initialize a SystemService to a actor address (can be used to start
    /// SyncActors, panic if already started
    pub fn set<A: SystemService + Actor<Context = Context<A>>>(&self, addr: Addr<A>) {
        {
            if let Ok(hm) = self.registry.lock() {
                if let Some(addr) = hm.get(&TypeId::of::<A>()) {
                    if addr.downcast_ref::<Addr<A>>().is_some() {
                        panic!("Actor already started");
                    }
                }
            } else {
                panic!("System registry lock is poisoned");
            }
        }

        if let Ok(mut hm) = self.registry.lock() {
            hm.insert(TypeId::of::<A>(), Box::new(addr));
        } else {
            panic!("System registry lock is poisoned");
        }
    }*/
}

impl Clone for SystemRegistry {
    fn clone(&self) -> Self {
        SystemRegistry {
            arbiter: self.arbiter.clone(),
            registry: Arc::clone(&self.registry),
        }
    }
}
