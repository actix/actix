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

pub use fut::{self, ActorFuture, ActorStream, WrapFuture, WrapStream};

pub use actor::{Actor, ActorState, FramedActor, Supervised,
                Handler, ResponseType, StreamHandler,
                ActorContext, AsyncContext, SpawnHandle};
pub use arbiter::Arbiter;
pub use address::{Address, SyncAddress, Subscriber};
pub use context::{Context, ContextFutureSpawner};
pub use framed::FramedContext;
pub use message::{Request, Response};
pub use system::System;
pub use supervisor::Supervisor;
pub use sync::{SyncContext, SyncArbiter};
pub use registry::{ArbiterService, SystemService};

pub mod actix {
    pub use utils::Condition;
}
