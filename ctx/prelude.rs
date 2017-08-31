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

pub use ctx::{
    Service, ContextAware, CtxMessage, CtxService, CtxServiceBuilder, CtxServiceStream};

pub use ctxframed::{
    FramedContextAware, CtxFramedResult, CtxFramedService, CtxFramedServiceBuilder,
    CtxFramedServiceFuture, CtxFramedServiceStream, CtxFramedServiceFutStream};
