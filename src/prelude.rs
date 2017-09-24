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
pub use address::Address;
pub use builder::Builder;
pub use context::{Context, CtxFutureSpawner, ServiceStream};
pub use framed::{CtxFramed, CtxFramedRead, CtxFramedWrite};
pub use message::{MessageResult, MessageTransport};
pub use sink::{SinkService, Sink, SinkContext, SinkResult};

pub use service::{DefaultMessage, Service, ServiceResult};
pub use service::{Message, MessageFuture, MessageHandler};

pub mod ctx {
    pub use task::Task;
}
