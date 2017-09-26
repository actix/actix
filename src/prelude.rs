//! The `ctx` prelude
//!
//! The purpose of this module is to alleviate imports of many common ctx traits
//! by adding a glob import to the top of ctx heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use actix::prelude::*;
//! ```

pub use fut::{self, ActorFuture, WrapFuture, IntoActorFuture};

pub use actor::{Actor, MessageHandler, StreamHandler};
pub use arbiter::{Arbiter, ArbiterAddress, StopArbiter};
pub use address::{Address, SyncAddress, Subscriber, AsyncSubscriber};
pub use builder::ActorBuilder;

pub use context::{Context, ActixFutureSpawner};
pub use framed::{ActixFramed, ActixFramedRead, ActixFramedWrite};
pub use message::{MessageFuture, MessageFutureResult, MessageFutureError, MessageResult};
pub use system::{System, SystemExit};

pub mod ctx {
    pub use sink::Sink;
    pub use utils::Condition;
}
