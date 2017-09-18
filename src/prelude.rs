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

pub use framed::{CtxFramed, CtxFramedRead, CtxFramedWrite};
pub use ctx::{CtxContext, CtxBuilder, CtxService, CtxServiceStream};
pub use sink::{SinkContext, CtxSinkService, CtxSink, CtxSinkResult};
