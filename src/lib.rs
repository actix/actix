//! # Actix is a rust actors framework
//!
//! [Actors](https://actix.github.io/actix/actix/trait.Actor.html) are
//! objects which encapsulate state and behavior, they communicate
//! exclusively by exchanging messages. Actix actors are implemented
//! on top of [Tokio](https://tokio.rs).  Multiple actors can run in
//! same thread. Actors can run in multiple threads using the
//! [`Arbiter`](struct.Arbiter.html) API. Actors exchange typed
//! messages.
//!
//! ## Documentation
//!
//! * [User Guide](https://actix.rs/book/actix/)
//! * [Chat on gitter](https://gitter.im/actix/actix)
//! * [GitHub repository](https://github.com/actix/actix)
//! * [Cargo package](https://crates.io/crates/actix)
//! * Minimum supported Rust version: 1.26 or later
//!
//! ## Features
//!
//! * Async/Sync actors.
//! * Actor communication in a local/thread context.
//! * Using Futures for asynchronous message handling.
//! * HTTP1/HTTP2 support ([actix-web](https://github.com/actix/actix-web))
//! * Actor supervision.
//! * Typed messages (No `Any` type). Generic messages are allowed.
//!
//! ## Package feature
//!
//! * `resolver` - enables dns resolver actor, `actix::actors::resolver`
//! * `signal` - enables signals handling actor
//!
//! ## Tokio runtime
//!
//! At the moment actix uses
//! [`current_thread`](https://docs.rs/tokio/0.1.13/tokio/runtime/current_thread/index.html) runtime.
//!
//! While it provides minimum overhead, it has its own limits:
//!
//! - You cannot use tokio's async file I/O, as it relies on blocking calls that are not available
//! in `current_thread`
//! - `Stdin`, `Stderr` and `Stdout` from `tokio::io` are the same as file I/O in that regard and
//! cannot be used in asynchronous manner in actix.
#[macro_use]
extern crate log;
extern crate bytes;
extern crate crossbeam_channel;
extern crate fnv;
#[cfg(unix)]
extern crate libc;
extern crate parking_lot;
extern crate smallvec;
extern crate uuid;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate futures;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_executor;
extern crate tokio_io;
extern crate tokio_reactor;
extern crate tokio_tcp;
extern crate tokio_timer;

#[macro_use]
extern crate failure;

#[cfg_attr(feature = "cargo-clippy", allow(useless_attribute))]
#[allow(unused_imports)]
#[macro_use]
extern crate actix_derive;

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
pub mod clock;
pub mod fut;
pub mod io;
pub mod msgs;
pub mod registry;
pub mod sync;
pub mod utils;

pub use actor::{
    Actor, ActorContext, ActorState, AsyncContext, Running, SpawnHandle, Supervised,
};
pub use address::{Addr, MailboxError, Recipient, WeakAddr};
pub use arbiter::{Arbiter, ArbiterBuilder};
pub use context::Context;
pub use fut::{ActorFuture, ActorStream, FinishStream, WrapFuture, WrapStream};
pub use handler::{
    ActorResponse, Handler, Message, MessageResult, Response, ResponseActFuture,
    ResponseFuture,
};
pub use registry::{ArbiterService, Registry, SystemRegistry, SystemService};
pub use stream::StreamHandler;
pub use supervisor::Supervisor;
pub use sync::{SyncArbiter, SyncContext};
pub use system::{System, SystemRunner};

#[doc(hidden)]
pub use context::ContextFutureSpawner;

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
    pub use actix_derive::*;

    pub use actor::{
        Actor, ActorContext, ActorState, AsyncContext, Running, SpawnHandle, Supervised,
    };
    pub use address::{
        Addr, MailboxError, Recipient, RecipientRequest, Request, SendError,
    };
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

    pub use actors;
    pub use dev;
    pub use fut;
    pub use io;
    pub use msgs;
    pub use utils::{Condition, IntervalFunc, TimerFunc};
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

    pub use prelude::*;

    pub use address::{Envelope, EnvelopeProxy, RecipientRequest, Request, ToEnvelope};
    pub mod channel {
        pub use address::channel::{channel, AddressReceiver, AddressSender};
    }
    pub use contextimpl::{AsyncContextParts, ContextFut, ContextParts};
    pub use handler::{MessageResponse, ResponseChannel};
    pub use mailbox::Mailbox;
    pub use registry::{Registry, SystemRegistry};
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
/// # extern crate actix;
/// # extern crate tokio_timer;
/// # extern crate futures;
/// # use futures::Future;
/// use std::time::{Duration, Instant};
/// use tokio_timer::Delay;
///
/// fn main() {
///   actix::run(
///       || Delay::new(Instant::now() + Duration::from_millis(100))
///            .map(|_| actix::System::current().stop())
///            .map_err(|_| ())
///   );
/// }
/// ```
///
/// # Panics
///
/// This function panics if the actix system is already running.
pub fn run<F, R>(f: F)
where
    F: FnOnce() -> R,
    R: futures::Future<Item = (), Error = ()> + 'static,
{
    if System::is_set() {
        panic!("System is already running");
    }

    let sys = System::new("Default");
    Arbiter::spawn(f());
    sys.run();
}

/// Spawns a future on the current arbiter.
///
/// # Panics
///
/// This function panics if the actix system is not running.
pub fn spawn<F>(f: F)
where
    F: futures::Future<Item = (), Error = ()> + 'static,
{
    if !System::is_set() {
        panic!("System is not running");
    }

    Arbiter::spawn(f);
}
