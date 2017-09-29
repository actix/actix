//! The `actix` prelude
//!
//! The purpose of this module is to alleviate imports of many common ctx traits
//! by adding a glob import to the top of ctx heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use actix::prelude::*;
//! ```

pub use fut::{self, ActorFuture, WrapFuture, IntoActorFuture};

pub use actor::{Actor, Supervised, MessageHandler, MessageResponse, StreamHandler};
pub use arbiter::Arbiter;
pub use address::{Address, SyncAddress, Subscriber, AsyncSubscriber};
pub use builder::ActorBuilder;
pub use context::{ActorState, Context, ContextFutureSpawner};
pub use framed::{ActixFramed, ActixFramedRead, ActixFramedWrite};
pub use message::{MessageFuture, MessageFutureResult, MessageFutureError, MessageResult};
pub use system::System;
pub use supervisor::Supervisor;
pub use registry::{ArbiterService, SystemService};

pub mod actix {
    pub use actors;
    pub use sink::Sink;
    pub use utils::Condition;
    pub use system::SystemExit;
    pub use arbiter::{Execute, StartActor, StopArbiter};
}
