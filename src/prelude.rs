//! The `ctx` prelude
//!
//! The purpose of this module is to alleviate imports of many common ctx traits
//! by adding a glob import to the top of ctx heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use ctx::prelude::*;
//! ```


pub use actor::Actor;
pub use address::{Address, Subscriber};

pub use fut::{self, CtxFuture, WrapFuture, IntoCtxFuture};
pub use builder::ServiceBuilder;
pub use context::{Context, CtxFutureSpawner, ServiceStream};
pub use framed::{CtxFramed, CtxFramedRead, CtxFramedWrite};
pub use message::{MessageFuture, MessageFutureResult, MessageFutureError, MessageResult};
pub use sink::{SinkService, Sink, SinkContext, SinkResult};

pub use service::{DefaultMessage, Service, ServiceResult};
pub use service::{Message, MessageHandler};

pub mod ctx {
    pub use waiter::Waiter;
    pub use system::{get_system, get_handle, init_system, System, SystemExit};
}
