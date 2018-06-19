//! Actors registry
//!
//! Actor can register itself as a service. Service can be defined as
//! `ArbiterService` which is unique per arbiter or `SystemService` which is
//! unique per system.
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::default::Default;
use std::hash::BuildHasherDefault;
use std::rc::Rc;
use std::sync::Arc;

use fnv::FnvHasher;
use parking_lot::ReentrantMutex;

use actor::{Actor, Supervised};
use address::Addr;
use arbiter::Arbiter;
use context::Context;
use supervisor::Supervisor;
use system::System;

type AnyMap = HashMap<TypeId, Box<Any>, BuildHasherDefault<FnvHasher>>;

/// Actors registry
///
/// Actor can register itself as a service. Service can be defined as
/// `ArbiterService` which is unique per arbiter or `SystemService` which is
/// unique per system.
///
/// If arbiter service is used outside of runnig arbiter, it panics.
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
/// #     System::current().stop();
///    }
/// }
///
/// struct MyActor2;
///
/// impl Actor for MyActor2 {
///    type Context = Context<Self>;
///
///    fn started(&mut self, _: &mut Context<Self>) {
///       // get MyActor1 addres from the registry
///       let act = Arbiter::registry().get::<MyActor1>();
///       act.do_send(Ping);
///    }
/// }
///
///
/// fn main() {
///     // initialize system
///     let code = System::run(|| {
///
///         // Start MyActor1 in new Arbiter
///         Arbiter::start(|_| {
///             MyActor2
///         });
///     });
/// }
/// ```
#[derive(Clone)]
pub struct Registry {
    registry: Rc<RefCell<AnyMap>>,
}

/// Trait defines arbiter's service.
#[allow(unused_variables)]
pub trait ArbiterService: Actor<Context = Context<Self>> + Supervised + Default {
    /// Construct and srtart arbiter service
    fn start_service() -> Addr<Self> {
        Supervisor::start(|ctx| {
            let mut act = Self::default();
            act.service_started(ctx);
            act
        })
    }

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
            registry: Rc::new(RefCell::new(HashMap::default())),
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
        let addr: Addr<A> = A::start_service();

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
            if addr.downcast_ref::<Addr<A>>().is_some() {
                panic!("Actor already started");
            }
        }

        self.registry.borrow_mut().insert(id, Box::new(addr));
    }
}

/// System wide actors registry
///
/// System registry serves same purpose as [Registry](struct.Registry.html),
/// except it is shared across all arbiters.
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
/// #       System::current().stop();
///     }
/// }
///
/// struct MyActor2;
///
/// impl Actor for MyActor2 {
///     type Context = Context<Self>;
///
///     fn started(&mut self, _: &mut Context<Self>) {
///         let act = System::current().registry().get::<MyActor1>();
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
/// }
/// ```
pub struct SystemRegistry {
    system: Addr<Arbiter>,
    registry: InnerRegistry,
}

type AnyMapSend = HashMap<TypeId, Box<Any + Send>, BuildHasherDefault<FnvHasher>>;
type InnerRegistry = Arc<ReentrantMutex<RefCell<AnyMapSend>>>;

/// Trait defines system's service.
#[allow(unused_variables)]
pub trait SystemService: Actor<Context = Context<Self>> + Supervised + Default {
    /// Construct and srtart system service
    fn start_service(sys: &Addr<Arbiter>) -> Addr<Self> {
        Supervisor::start_in_arbiter(sys, |ctx| {
            let mut act = Self::default();
            act.service_started(ctx);
            act
        })
    }

    /// Method is called during service initialization.
    fn service_started(&mut self, ctx: &mut Context<Self>) {}

    /// Get actor's address from system registry
    fn from_registry() -> Addr<Self> {
        System::with_current(|sys| sys.registry().get::<Self>())
    }
}

impl SystemRegistry {
    pub(crate) fn new(system: Addr<Arbiter>) -> Self {
        SystemRegistry {
            system,
            registry: Arc::new(ReentrantMutex::new(RefCell::new(HashMap::default()))),
        }
    }

    /// Return address of the service. If service actor is not running
    /// it get started in the system.
    pub fn get<A: SystemService + Actor<Context = Context<A>>>(&self) -> Addr<A> {
        let hm = self.registry.lock();
        if let Some(addr) = hm.borrow().get(&TypeId::of::<A>()) {
            match addr.downcast_ref::<Addr<A>>() {
                Some(addr) => return addr.clone(),
                None => panic!("Got unknown value: {:?}", addr),
            }
        }

        let addr = A::start_service(&self.system);
        hm.borrow_mut()
            .insert(TypeId::of::<A>(), Box::new(addr.clone()));
        addr
    }
}

impl Clone for SystemRegistry {
    fn clone(&self) -> Self {
        SystemRegistry {
            system: self.system.clone(),
            registry: Arc::clone(&self.registry),
        }
    }
}
