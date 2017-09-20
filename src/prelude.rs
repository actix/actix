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
pub use builder::Builder;
pub use context::{Context, ServiceStream};
pub use framed::{CtxFramed, CtxFramedRead, CtxFramedWrite};
pub use service::Service;
pub use sink::{SinkService, Sink, SinkContext, SinkResult};
