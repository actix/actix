//! # Actix is a rust actors framework.
//!
//! [Actors](https://actix.github.io/actix/actix/trait.Actor.html) are objects
//! which encapsulate state and behavior, they communicate exclusively
//! by exchanging messages. Actix actors are implemented on top of [Tokio](https://tokio.rs).
//! Multiple actors could run in same thread. Actors could run in multiple
//! threads with support of [`Arbiter`](https://actix.github.io/actix/actix/struct.Arbiter.html).
//! Actors exchange typed messages.
//!
//! ## Documentation
//!
//! * [User Guide](http://actix.github.io/actix/guide/)
//! * [Chat on gitter](https://gitter.im/actix/actix)
//! * [GitHub repository](https://github.com/actix/actix)
//! * [Cargo package](https://crates.io/crates/actix)
//! * Minimum supported Rust version: 1.21 or later
//!
//! ## Features
//!
//! * Async/Sync actors.
//! * Actor communication in a local/thread context.
//! * Using Futures for asynchronous message handling.
//! * HTTP1/HTTP2 support ([actix-web](https://github.com/actix/actix-web))
//! * Actor supervision.
//! * Typed messages (No `Any` type). Generic messages are allowed.

#[macro_use]
extern crate log;
extern crate crossbeam_channel;
extern crate libc;
extern crate smallvec;
extern crate uuid;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_signal;
extern crate tokio_tcp;
extern crate tokio_timer;
extern crate trust_dns_resolver;

#[macro_use]
extern crate failure;

#[cfg_attr(feature = "cargo-clippy", allow(useless_attribute))]
#[allow(unused_imports)]
#[macro_use]
extern crate actix_derive;

#[cfg(test)]
extern crate bytes;

#[doc(hidden)]
pub use actix_derive::*;

mod actor;
mod arbiter;
mod context;
mod contextimpl;
mod contextitems;
mod handler;
mod stream;
mod supervisor;
mod system;

mod address;
mod mailbox;

pub mod actors;
pub mod fut;
pub mod io;
pub mod msgs;
pub mod registry;
pub mod sync;
pub mod utils;

pub use actor::{
    Actor, ActorContext, ActorState, AsyncContext, Running, SpawnHandle, Supervised,
};
pub use address::{ActorAddress, Addr, MailboxError, Recipient, Syn, Unsync};
pub use arbiter::Arbiter;
pub use context::Context;
pub use fut::{ActorFuture, ActorStream, FinishStream, WrapFuture, WrapStream};
pub use handler::{
    ActorResponse, Handler, Message, MessageResult, Response, ResponseActFuture,
    ResponseFuture,
};
pub use stream::StreamHandler;
pub use supervisor::Supervisor;
pub use sync::{SyncArbiter, SyncContext};
pub use system::{System, SystemRunner};

#[doc(hidden)]
pub use context::ContextFutureSpawner;

pub mod prelude {
    //! The `actix` prelude
    //!
    //! The purpose of this module is to alleviate imports of many common actix
    //! traits by adding a glob import to the top of actix heavy modules:
    //!
    //! ```
    //! # #![allow(unused_imports)]
    //! use actix::prelude::*;
    //! ```

    #[doc(hidden)]
    pub use actix_derive::*;

    pub use actor::{
        Actor, ActorContext, ActorState, AsyncContext, Running, SpawnHandle, Supervised,
    };
    pub use address::{Addr, MailboxError, Recipient, SendError, Syn, Unsync};
    pub use arbiter::Arbiter;
    pub use context::{Context, ContextFutureSpawner};
    pub use fut::{ActorFuture, ActorStream, WrapFuture, WrapStream};
    pub use handler::{
        ActorResponse, Handler, Message, MessageResult, Response, ResponseActFuture,
        ResponseFuture,
    };
    pub use registry::{ArbiterService, SystemService};
    pub use stream::StreamHandler;
    pub use supervisor::Supervisor;
    pub use sync::{SyncArbiter, SyncContext};
    pub use system::System;

    pub mod actix {
        pub use actors;
        pub use address::ActorAddress;
        pub use dev;
        pub use fut;
        pub use io;
        pub use msgs;
        pub use prelude::*;
        pub use registry::{ArbiterService, SystemService};
        pub use utils::Condition;
    }
}

pub mod dev {
    //! The `actix` prelude for library developers
    //!
    //! The purpose of this module is to alleviate imports of many common actix
    //! traits by adding a glob import to the top of actix heavy modules:
    //!
    //! ```
    //! # #![allow(unused_imports)]
    //! use actix::dev::*;
    //! ```

    pub use prelude::actix::*;
    pub use prelude::*;

    pub use address::{
        ActorAddress, Request, SyncEnvelope, SyncRecipientRequest, ToEnvelope,
        UnsyncRecipientRequest,
    };
    pub use address::{
        Destination, MessageDestination, MessageDestinationTransport, MessageRecipient,
    };
    pub use contextimpl::ContextImpl;
    pub use handler::{MessageResponse, ResponseChannel};
}
