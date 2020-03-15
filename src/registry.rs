//! Actors registry
//!
//! An Actor can register itself as a service. A Service can be defined as an
//! `ArbiterService`, which is unique per arbiter, or a `SystemService`, which
//! is unique per system.
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::default::Default;
use std::rc::Rc;

use actix_rt::{Arbiter, System};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::actor::{Actor, Supervised};
use crate::address::Addr;
use crate::context::Context;
use crate::supervisor::Supervisor;

type AnyMap = HashMap<TypeId, Box<dyn Any>>;

/// Actors registry
///
/// An Actor can register itself as a service. A Service can be defined as an
/// `ArbiterService`, which is unique per arbiter, or a `SystemService`, which
/// is unique per system.
///
/// If an arbiter service is used outside of a running arbiter, it panics.
///
/// # Example
///
/// ```rust
/// use actix::prelude::*;
///
/// #[derive(Message)]
/// #[rtype(result = "()")]
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
///       // get MyActor1 address from the registry
///       let act = MyActor1::from_registry();
///       act.do_send(Ping);
///    }
/// }
///
/// fn main() {
///     // initialize system
///     let code = System::run(|| {
///         // Start MyActor2 in new Arbiter
///         Arbiter::new().exec_fn(|| {
///             MyActor2.start();
///         });
///     });
/// }
/// ```
#[derive(Clone)]
pub struct Registry {
    registry: Rc<RefCell<AnyMap>>,
}

thread_local! {
    static AREG: Registry = {
        Registry {
            registry: Rc::new(RefCell::new(AnyMap::new()))
        }
    };
}

/// Trait defines arbiter's service.
#[allow(unused_variables)]
pub trait ArbiterService: Actor<Context = Context<Self>> + Supervised + Default {
    /// Construct and start arbiter service
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
        AREG.with(|reg| reg.get())
    }
}

impl Registry {
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

    /// Check if actor is in registry, if so, return its address
    pub fn query<A: ArbiterService + Actor<Context = Context<A>>>(
        &self,
    ) -> Option<Addr<A>> {
        let id = TypeId::of::<A>();
        if let Some(addr) = self.registry.borrow().get(&id) {
            if let Some(addr) = addr.downcast_ref::<Addr<A>>() {
                return Some(addr.clone());
            }
        }
        None
    }

    /// Add new actor to the registry by address, panic if actor is already running
    pub fn set<A: ArbiterService + Actor<Context = Context<A>>>(addr: Addr<A>) {
        AREG.with(|reg| {
            let id = TypeId::of::<A>();
            if let Some(addr) = reg.registry.borrow().get(&id) {
                if addr.downcast_ref::<Addr<A>>().is_some() {
                    panic!("Actor already started");
                }
            }

            reg.registry.borrow_mut().insert(id, Box::new(addr));
        })
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
/// use actix::prelude::*;
///
/// #[derive(Message)]
/// #[rtype(result = "()")]
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
///         let act = MyActor1::from_registry();
///         act.do_send(Ping);
///     }
/// }
///
/// fn main() {
///     // initialize system
///     let code = System::run(|| {
///         // Start MyActor2
///         let addr = MyActor2.start();
///     });
/// }
/// ```
#[derive(Debug)]
pub struct SystemRegistry {
    system: Arbiter,
    registry: HashMap<TypeId, Box<dyn Any + Send>>,
}

static SREG: Lazy<Mutex<HashMap<usize, SystemRegistry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Trait defines system's service.
#[allow(unused_variables)]
pub trait SystemService: Actor<Context = Context<Self>> + Supervised + Default {
    /// Construct and start system service
    fn start_service(sys: &Arbiter) -> Addr<Self> {
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
        System::with_current(|sys| {
            let mut sreg = SREG.lock();
            let reg = sreg
                .entry(sys.id())
                .or_insert_with(|| SystemRegistry::new(sys.arbiter().clone()));

            if let Some(addr) = reg.registry.get(&TypeId::of::<Self>()) {
                if let Some(addr) = addr.downcast_ref::<Addr<Self>>() {
                    return addr.clone();
                }
            }

            let addr = Self::start_service(System::current().arbiter());
            reg.registry
                .insert(TypeId::of::<Self>(), Box::new(addr.clone()));
            addr
        })
    }
}

impl SystemRegistry {
    pub(crate) fn new(system: Arbiter) -> Self {
        Self {
            system,
            registry: HashMap::default(),
        }
    }

    /// Return address of the service. If service actor is not running
    /// it get started in the system.
    pub fn get<A: SystemService + Actor<Context = Context<A>>>(&mut self) -> Addr<A> {
        if let Some(addr) = self.registry.get(&TypeId::of::<A>()) {
            match addr.downcast_ref::<Addr<A>>() {
                Some(addr) => return addr.clone(),
                None => panic!("Got unknown value: {:?}", addr),
            }
        }

        let addr = A::start_service(&self.system);
        self.registry
            .insert(TypeId::of::<A>(), Box::new(addr.clone()));
        addr
    }

    /// Check if actor is in registry, if so, return its address
    pub fn query<A: SystemService + Actor<Context = Context<A>>>(
        &self,
    ) -> Option<Addr<A>> {
        if let Some(addr) = self.registry.get(&TypeId::of::<A>()) {
            match addr.downcast_ref::<Addr<A>>() {
                Some(addr) => return Some(addr.clone()),
                None => return None,
            }
        }

        None
    }

    /// Add new actor to the registry by address, panic if actor is already running
    pub fn set<A: SystemService + Actor<Context = Context<A>>>(addr: Addr<A>) {
        System::with_current(|sys| {
            let mut sreg = SREG.lock();
            let reg = sreg
                .entry(sys.id())
                .or_insert_with(|| SystemRegistry::new(sys.arbiter().clone()));

            if let Some(addr) = reg.registry.get(&TypeId::of::<A>()) {
                if addr.downcast_ref::<Addr<A>>().is_some() {
                    panic!("Actor already started");
                }
            }

            reg.registry.insert(TypeId::of::<A>(), Box::new(addr));
        })
    }
}
