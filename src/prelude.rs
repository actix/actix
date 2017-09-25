//! The `ctx` prelude
//!
//! The purpose of this module is to alleviate imports of many common ctx traits
//! by adding a glob import to the top of ctx heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use ctx::prelude::*;
//! ```

pub use fut::{self, CtxFuture, WrapFuture, IntoCtxFuture};

pub use actor::Actor;
pub use arbiter::{Arbiter, StopArbiter, ArbiterAddress};
pub use address::{Address, SyncAddress, Subscriber, AsyncSubscriber};
pub use builder::ServiceBuilder;

pub use context::{Context, CtxFutureSpawner};
pub use framed::{CtxFramed, CtxFramedRead, CtxFramedWrite};
pub use message::{MessageFuture, MessageFutureResult, MessageFutureError, MessageResult};
pub use service::{Message, MessageHandler, DefaultMessage, Service, ServiceResult};
pub use system::{System, SystemExit};

pub mod ctx {
    pub use sink::Sink;
    pub use utils::Condition;
}
