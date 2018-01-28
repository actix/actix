//! # Actix is a rust actor framework.
//!
//! [Actors](https://actix.github.io/actix/actix/trait.Actor.html) are objects
//! which encapsulate state and behavior, they communicate exclusively
//! by exchanging messages. Actix actors are implemented on top of [Tokio](https://tokio.rs).
//! Mutiple actors could run in same thread. Actors could run in multiple threads
//! with support of [`Arbiter`](https://actix.github.io/actix/actix/struct.Arbiter.html).
//! Actors exchange typed messages.
//!
//! ## Features
//!
//! * Async/Sync actors.
//! * Actor communication in a local/thread context.
//! * Using Futures for asynchronous message handling.
//! * HTTP1/HTTP2 support ([actix-web](https://github.com/actix/actix-web))
//! * Actor supervision.
//! * Typed messages (No `Any` type). Generic messages are allowed.
//! * Minimum supported Rust version: 1.20 or later

#[macro_use]
extern crate log;
extern crate libc;
extern crate uuid;
extern crate smallvec;
extern crate crossbeam_channel;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate tokio_signal;
extern crate trust_dns_resolver;

#[macro_use]
extern crate failure;

#[cfg_attr(feature="cargo-clippy", allow(useless_attribute))]
#[allow(unused_imports)]
#[macro_use]
extern crate actix_derive;

#[cfg(test)]
extern crate bytes;

#[doc(hidden)]
pub use actix_derive::*;

mod actor;
mod arbiter;
mod context;
mod contextimpl;
mod contextitems;
mod framed;
mod handler;
mod registry;
mod system;
mod supervisor;

mod address;
mod contextaddress;

pub mod fut;
pub mod actors;
pub mod msgs;
pub mod sync;
pub mod utils;

pub use fut::{ActorFuture, ActorStream, WrapFuture, WrapStream, FinishStream};
pub use actor::{Actor, ActorState, Supervised,
                ActorContext, AsyncContext, SpawnHandle};
pub use handler::{Handler, StreamHandler,
                  Response, ResponseType, MessageResult, ResponseFuture};
pub use arbiter::Arbiter;
pub use address::{Address, ActorAddress, SyncAddress, Subscriber, ToEnvelope};
pub use context::Context;
pub use framed::{FramedHandler, FramedCell};
pub use sync::{SyncContext, SyncArbiter};
pub use registry::{Registry, SystemRegistry, ArbiterService, SystemService};
pub use system::{System, SystemRunner};
pub use supervisor::Supervisor;

#[doc(hidden)]
pub use context::ContextFutureSpawner;

pub mod prelude {
//! The `actix` prelude
//!
//! The purpose of this module is to alleviate imports of many common actix traits
//! by adding a glob import to the top of actix heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use actix::prelude::*;
//! ```

    #[doc(hidden)]
    pub use actix_derive::*;

    pub use fut::{ActorFuture, ActorStream, WrapFuture, WrapStream};
    pub use actor::{Actor, ActorState, ActorContext, AsyncContext, Supervised, SpawnHandle};
    pub use arbiter::Arbiter;
    pub use address::{Address, SyncAddress};
    pub use context::{Context, ContextFutureSpawner};
    pub use framed::{FramedCell, FramedHandler};
    pub use handler::{Handler, StreamHandler,
                      Response, ResponseType, MessageResult, ResponseFuture};
    pub use system::System;
    pub use sync::{SyncContext, SyncArbiter};
    pub use supervisor::Supervisor;

    pub mod actix {
        pub use prelude::*;
        pub use fut;
        pub use msgs;
        pub use address::{Subscriber, ActorAddress};
        pub use registry::{ArbiterService, SystemService};
        pub use utils::Condition;
    }
}

pub mod dev {
//! The `actix` prelude for library developers
//!
//! The purpose of this module is to alleviate imports of many common actix traits
//! by adding a glob import to the top of actix heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use actix::dev::*;
//! ```

    pub use prelude::*;
    pub use prelude::actix::*;

    pub use context::AsyncContextApi;
    pub use contextimpl::ContextImpl;
    pub use address::{ActorAddress, SendError,
                      Envelope, ToEnvelope, RemoteEnvelope, Request, LocalRequest, LocalFutRequest};
}
