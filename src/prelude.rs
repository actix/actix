//! The `actix` prelude
//!
//! The purpose of this module is to alleviate imports of many common actix traits
//! by adding a glob import to the top of actix heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use actix::prelude::*;
//! ```

pub use msgs;
pub use fut::{self, ActorFuture, WrapFuture, IntoActorFuture};

pub use actor::{Actor, Supervised, MessageHandler, MessageResponse, StreamHandler};
pub use arbiter::Arbiter;
pub use address::{Address, SyncAddress, Subscriber, AsyncSubscriber};
pub use builder::ActorBuilder;
pub use context::{ActorState, Context, ContextFutureSpawner};
pub use framed::{ActixFramed, ActixFramedRead, ActixFramedWrite};
pub use message::{Request, Response, ResponseItem, ResponseError};
pub use system::System;
pub use supervisor::Supervisor;
pub use registry::{ArbiterService, SystemService};

pub mod actix {
    pub use sink::Sink;
    pub use utils::Condition;
}
