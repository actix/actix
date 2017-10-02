//! # Actix is a rust actor system framework.
//!
//! [Actors](https://fafhrd91.github.io/actix/actix/trait.Actor.html) are objects
//! which encapsulate state and behavior, they communicate exclusively
//! by exchanging messages. Actix actors are implemented on top of [Tokio](https://tokio.rs).
//! Mutiple actors could run in same thread. Actors could run in multiple threads
//! with suppoprt of [`Arbiter`](https://fafhrd91.github.io/actix/actix/struct.Arbiter.html).
//! Actors exchange typed messages. Actix does not use any unstable rust features and
//! can be compiled with state rust compiler.
//!
//! ## Features
//!
//! * Typed messages (No `Any` type). Generic messages are allowed.
//! * Actor communication in a local/thread context.
//! * Actor supervision.
//! * Using Futures for asynchronous message handling.
//! * Compiles with stable rust

#[macro_use]
extern crate log;
extern crate uuid;

extern crate futures;
extern crate tokio_io;
extern crate tokio_core;

#[cfg(feature="signal")]
extern crate tokio_signal;
#[cfg(feature="signal")]
extern crate libc;

mod actor;
mod arbiter;
mod address;
mod builder;
mod context;
mod envelope;
mod framed;
mod queue;
mod message;
mod registry;
mod system;
mod supervisor;
mod utils;

pub mod fut;
pub mod prelude;
pub mod actors;
pub mod msgs;

pub use actor::{Actor, ActorState, Supervised,
                Handler, ResponseType, StreamHandler,
                BaseContext, AsyncContext};
pub use arbiter::Arbiter;
pub use address::{Address, SyncAddress, Subscriber};
pub use builder::ActorBuilder;
pub use context::{Context, ContextFutureSpawner};
pub use framed::FramedContext;
pub use message::{Request, Response};
pub use registry::{Registry, SystemRegistry, ArbiterService, SystemService};
pub use system::{System, SystemRunner};
pub use utils::Condition;
pub use supervisor::Supervisor;
