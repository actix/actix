#![feature(specialization)]

#[macro_use]
extern crate bitflags;

extern crate futures;
extern crate tokio_io;
extern crate tokio_core;

mod ctx;
mod ctxframed;
mod framed;

pub mod fut;
pub mod prelude;

pub use fut::CtxFuture;
pub use ctx::{
    Service, ContextAware, CtxMessage, CtxService, CtxServiceBuilder, CtxServiceStream};
pub use ctxframed::{
    FramedContextAware, CtxFramedResult, CtxFramedError, CtxFramedService,
    CtxFramedServiceBuilder, CtxFramedServiceFuture, CtxFramedServiceStream,
    CtxFramedServiceFutStream};
