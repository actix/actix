//! # Actix is a rust actor system framework.
//!
//!
//!

#[macro_use]
extern crate log;
#[macro_use]
extern crate bitflags;
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
mod framed;
mod message;
mod registry;
mod sink;
mod system;
mod utils;

pub mod fut;
pub mod prelude;
pub mod actors;

pub use actor::{Actor, MessageHandler, StreamHandler};
pub use address::{Address, SyncAddress, Subscriber, AsyncSubscriber};
pub use arbiter::{Arbiter, StopArbiter, ArbiterAddress};
pub use builder::ActorBuilder;
pub use context::{Context, ActixFutureSpawner};
pub use framed::{ActixFramed, ActixFramedRead, ActixFramedWrite};
pub use message::{MessageResult, MessageFuture, MessageFutureResult, MessageFutureError};
pub use registry::{Registry, SystemRegistry};
pub use sink::Sink;
pub use system::{System, SystemExit, SystemRunner};
pub use utils::Condition;
