//! Actix is an actor framework.
//!
//! [Actors](./trait.Actor.html) are
//! objects which encapsulate state and behavior, they communicate
//! exclusively by exchanging messages. Actix actors are implemented
//! on top of [Tokio](https://tokio.rs).  Multiple actors can run in
//! same thread. Actors can run in multiple threads using the
//! [`Arbiter`](struct.Arbiter.html) API. Actors exchange typed
//! messages.
//!
//! ## Other Documentation
//! - [User Guide](https://actix.rs/book/actix/)
//! - [Community Chat on Gitter](https://gitter.im/actix/actix)
//!
//! ## Features
//! - Async/Sync actors
//! - Actor communication in a local/thread context
//! - Using Futures for asynchronous message handling
//! - Actor supervision
//! - Typed messages (No `Any` type). Generic messages are allowed
//! - Runs on stable Rust 1.40+
//!
//! ## Package feature
//! * `resolver` - enables DNS resolver actor; see [resolver](./actors/resolver/index.html) module

#![allow(clippy::needless_doctest_main)]
#![deny(nonstandard_style, rust_2018_idioms)]
#![warn(deprecated_in_future, trivial_casts, trivial_numeric_casts)]
// TODO: temporary allow deprecated until resolver actor is removed.
#![allow(deprecated)]

#[cfg(doctest)]
doc_comment::doctest!("../README.md");

mod actor;
mod context;
mod contextimpl;
mod contextitems;
mod handler;
mod stream;
mod supervisor;

mod address;
mod mailbox;

pub mod actors;
pub mod clock;
pub mod fut;
pub mod io;
pub mod registry;
pub mod sync;
pub mod utils;

#[cfg(feature = "macros")]
pub use actix_derive::{main, test, Message, MessageResponse};
pub use actix_rt::{spawn, Arbiter, ArbiterHandle, System, SystemRunner};

pub use crate::actor::{
    Actor, ActorContext, ActorState, AsyncContext, Running, SpawnHandle, Supervised,
};
pub use crate::address::{Addr, MailboxError, Recipient, WeakAddr, WeakRecipient};
pub use crate::context::Context;
pub use crate::fut::{
    ActorFuture, ActorFutureExt, ActorStream, ActorStreamExt, ActorTryFuture,
    ActorTryFutureExt, WrapFuture, WrapStream,
};
pub use crate::handler::{
    ActorResponse, AtomicResponse, Handler, Message, MessageResult, Response,
    ResponseActFuture, ResponseFuture,
};
pub use crate::registry::{ArbiterService, Registry, SystemRegistry, SystemService};
pub use crate::stream::StreamHandler;
pub use crate::supervisor::Supervisor;
pub use crate::sync::{SyncArbiter, SyncContext};

#[doc(hidden)]
pub use crate::context::ContextFutureSpawner;

pub mod prelude {
    //! The `actix` prelude.
    //!
    //! The purpose of this module is to alleviate imports of many common actix
    //! traits by adding a glob import to the top of actix heavy modules:
    //!
    //! ```
    //! # #![allow(unused_imports)]
    //! use actix::prelude::*;
    //! ```

    #[doc(hidden)]
    #[cfg(feature = "macros")]
    pub use actix_derive::{Message, MessageResponse};

    pub use actix_rt::{Arbiter, ArbiterHandle, System, SystemRunner};

    pub use crate::actor::{
        Actor, ActorContext, ActorState, AsyncContext, Running, SpawnHandle, Supervised,
    };
    pub use crate::address::{
        Addr, MailboxError, Recipient, RecipientRequest, Request, SendError,
    };
    pub use crate::context::{Context, ContextFutureSpawner};
    pub use crate::fut::{
        ActorFuture, ActorFutureExt, ActorStream, ActorStreamExt, ActorTryFuture,
        ActorTryFutureExt, WrapFuture, WrapStream,
    };
    pub use crate::handler::{
        ActorResponse, AtomicResponse, Handler, Message, MessageResult, Response,
        ResponseActFuture, ResponseFuture,
    };
    pub use crate::registry::{ArbiterService, SystemService};
    pub use crate::stream::StreamHandler;
    pub use crate::supervisor::Supervisor;
    pub use crate::sync::{SyncArbiter, SyncContext};

    pub use crate::actors;
    pub use crate::dev;
    pub use crate::fut;
    pub use crate::io;
    pub use crate::utils::{Condition, IntervalFunc, TimerFunc};

    // TODO: remove Stream re-export when it reaches std
    pub use futures_core::stream::Stream;
}

pub mod dev {
    //! The `actix` prelude for library developers.
    //!
    //! The purpose of this module is to alleviate imports of many common actix
    //! traits by adding a glob import to the top of actix heavy modules:
    //!
    //! ```
    //! # #![allow(unused_imports)]
    //! use actix::dev::*;
    //! ```

    pub use crate::prelude::*;

    pub use crate::address::{Envelope, EnvelopeProxy, RecipientRequest, Request, ToEnvelope};
    pub mod channel {
        pub use crate::address::channel::{channel, AddressReceiver, AddressSender};
    }
    pub use crate::contextimpl::{AsyncContextParts, ContextFut, ContextParts};
    pub use crate::handler::{MessageResponse, OneshotSender};
    pub use crate::mailbox::Mailbox;
    pub use crate::registry::{Registry, SystemRegistry};
}

/// Starts the system and executes the supplied future.
///
/// This function does the following:
///
/// * Creates and starts the actix system with default configuration.
/// * Spawns the given future onto the current arbiter.
/// * Blocks the current thread until the system shuts down.
///
/// The `run` function returns when the `System::current().stop()`
/// method gets called.
///
/// # Examples
///
/// ```
/// use std::time::{Duration, Instant};
/// use actix_rt::time::sleep;
///
/// fn main() {
///   actix::run(async move {
///       sleep(Duration::from_millis(100)).await;
///       actix::System::current().stop();
///   });
/// }
/// ```
///
/// # Panics
///
/// This function panics if the actix system is already running.
#[allow(clippy::unit_arg)]
pub fn run<R>(f: R) -> std::io::Result<()>
where
    R: std::future::Future<Output = ()> + 'static,
{
    Ok(actix_rt::System::new().block_on(f))
}
