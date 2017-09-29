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

extern crate bytes;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_io;
extern crate tokio_core;

#[cfg(feature="signal")]
extern crate tokio_signal;
#[cfg(feature="signal")]
extern crate libc;

mod actor;
mod arbiter;
mod address;
mod sync_address;
mod builder;
mod context;
mod message;
mod queue;
mod registry;
mod sink;
mod system;
mod supervisor;
mod utils;

pub mod fut;
pub mod prelude;
pub mod actors;
pub mod framed;

pub use actor::{Actor, Supervised, MessageHandler, MessageResponse, StreamHandler};
pub use address::{Address, SyncAddress, Subscriber, AsyncSubscriber};
pub use arbiter::{Arbiter, Execute, StartActor, StopArbiter};
pub use builder::ActorBuilder;
pub use context::{ActorState, Context, ContextFutureSpawner};
pub use message::{MessageResult, MessageFuture, MessageFutureResult, MessageFutureError};
pub use registry::{Registry, SystemRegistry, ArbiterService, SystemService};
pub use sink::Sink;
pub use system::{System, SystemExit, SystemRunner};
pub use utils::Condition;
pub use supervisor::Supervisor;
