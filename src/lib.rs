#![feature(conservative_impl_trait)]

#[macro_use]
extern crate log;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;

extern crate bytes;
#[macro_use]
extern crate futures;
#[macro_use]
extern crate tokio_io;
extern crate tokio_core;

mod actor;
mod address;
mod builder;
mod context;
mod service;
mod message;
mod sink;
mod framed;
mod system;
mod waiter;

pub mod fut;
pub mod prelude;

pub use actor::Actor;
pub use fut::CtxFuture;
pub use builder::ServiceBuilder;
pub use context::{Context, ServiceStream};
pub use framed::{CtxFramed, CtxFramedRead, CtxFramedWrite};
pub use sink::{SinkContext, Sink, SinkService, SinkResult};

pub use service::{Item, Service, ServiceResult, Message, MessageFuture};

pub use waiter::Waiter;
pub use system::{get_system, get_handle, init_system, System, SystemExit};
