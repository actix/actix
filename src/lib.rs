#![feature(conservative_impl_trait)]

#[macro_use]
extern crate log;

extern crate bytes;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_io;
extern crate tokio_core;
extern crate boxfnonce;

mod address;
mod builder;
mod context;
mod service;
mod message;
mod sink;
mod framed;

pub mod fut;
pub mod prelude;

pub use fut::CtxFuture;
pub use builder::Builder;
pub use context::{Context, ServiceStream};
pub use framed::{CtxFramed, CtxFramedRead, CtxFramedWrite};
pub use sink::{SinkContext, Sink, SinkService, SinkResult};
pub use service::{Item, Service};
pub use message::MessageTransport;
