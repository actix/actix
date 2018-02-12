//! An actor implementation of Unix signal handling
//!
//! This module implements asynchronous signal handling for Actix. For each signal
//! `ProcessSignals` actor sends `Signal` message to all subscriber. To subscriber,
//! send `Subscribe` message to `ProcessSignals` actor.
//!
//! # Examples
//!
//! ```rust
//! # extern crate actix;
//! use actix::prelude::*;
//! use actix::actors::signal;
//!
//! struct Signals;
//!
//! impl Actor for Signals {
//!     type Context = Context<Self>;
//! }
//!
//! // Shutdown system on and of `SIGINT`, `SIGTERM`, `SIGQUIT` signals
//! impl Handler<signal::Signal> for Signals {
//!     type Result = ();
//!
//!     fn handle(&mut self, msg: signal::Signal, _: &mut Context<Self>) {
//!         match msg.0 {
//!             signal::SignalType::Int => {
//!                 println!("SIGINT received, exiting");
//!                 Arbiter::system().send(actix::msgs::SystemExit(0));
//!             },
//!             signal::SignalType::Hup => {
//!                 println!("SIGHUP received, reloading");
//!             },
//!             signal::SignalType::Term => {
//!                 println!("SIGTERM received, stopping");
//!                 Arbiter::system().send(actix::msgs::SystemExit(0));
//!             },
//!             signal::SignalType::Quit => {
//!                 println!("SIGQUIT received, exiting");
//!                 Arbiter::system().send(actix::msgs::SystemExit(0));
//!             }
//!             _ => (),
//!         }
//!     }
//! }
//!
//! fn main() {
//!    // initialize system
//!    let sys = System::new("test");
//!
//!    // Start signals handler
//!    let addr: Addr<Sync<_>> = Signals.start();
//!
//!    // send SIGTERM
//!    std::thread::spawn(move || {
//!       // emulate SIGNTERM
//!       addr.send(signal::Signal(signal::SignalType::Term));
//!    });
//!
//!    // Run system, this function blocks until system runs
//!    let code = sys.run();
//!    std::process::exit(code);
//! }
//! ```
use std;
use libc;
use futures::{Future, Stream};
use tokio_signal;
#[cfg(unix)]
use tokio_signal::unix;

use prelude::*;

/// Different types of process signals
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum SignalType {
    /// SIGHUP
    Hup,
    /// SIGINT
    Int,
    /// SIGTERM
    Term,
    /// SIGQUIT
    Quit,
    /// SIGCHILD
    Child,
}

impl ResponseType for SignalType {
    type Item = ();
    type Error = ();
}

/// Process signal message
pub struct Signal(pub SignalType);

impl ResponseType for Signal {
    type Item = ();
    type Error = ();
}

/// An actor implementation of Unix signal handling
pub struct ProcessSignals {
    subscribers: Vec<SyncSubscriber<Signal>>,
}

impl Default for ProcessSignals {
    fn default() -> Self {
        ProcessSignals{subscribers: Vec::new()}
    }
}

impl Actor for ProcessSignals {
    type Context = Context<Self>;
}

impl actix::Supervised for ProcessSignals {}

impl actix::SystemService for ProcessSignals {

    fn service_started(&mut self, ctx: &mut Self::Context) {
        let handle = actix::Arbiter::handle();

        // SIGINT
        tokio_signal::ctrl_c(handle).map_err(|_| ())
            .actfuture()
            .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>|
                 ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Int)))
            .spawn(ctx);

        #[cfg(unix)]
        {
            // SIGHUP
            unix::Signal::new(libc::SIGHUP, handle)
                .actfuture()
                .drop_err()
                .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>|
                     ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Hup)))
                .spawn(ctx);

            // SIGTERM
            unix::Signal::new(libc::SIGTERM, handle).map_err(|_| ())
                .actfuture()
                .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>|
                     ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Term)))
                .spawn(ctx);

            // SIGQUIT
            unix::Signal::new(libc::SIGQUIT, handle).map_err(|_| ())
                .actfuture()
                .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>|
                     ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Quit)))
                .spawn(ctx);

            // SIGCHLD
            unix::Signal::new(libc::SIGCHLD, handle).map_err(|_| ())
                .actfuture()
                .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>|
                     ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Child)))
                .spawn(ctx);
        }
    }
}

#[doc(hidden)]
impl Handler<SignalType> for ProcessSignals {
    type Result = ();

    fn handle(&mut self, sig: SignalType, _: &mut Self::Context) {
        let subscribers = std::mem::replace(&mut self.subscribers, Vec::new());
        for subscr in subscribers {
            if subscr.send(Signal(sig)).is_ok() {
                self.subscribers.push(subscr);
            }
        }
    }
}

/// Subscribe to process signals.
pub struct Subscribe(pub SyncSubscriber<Signal>);

impl actix::ResponseType for Subscribe {
    type Item = ();
    type Error = ();
}

/// Add subscriber for signals
impl actix::Handler<Subscribe> for ProcessSignals {
    type Result = ();

    fn handle(&mut self, msg: Subscribe, _: &mut Self::Context) {
        self.subscribers.push(msg.0);
    }
}

/// Default signals handler. This actor sends `SystemExit` message to `System` actor
/// for each of `SIGINT`, `SIGTERM`, `SIGQUIT` signals.
pub struct DefaultSignalsHandler;

impl Default for DefaultSignalsHandler {
    fn default() -> Self {
        DefaultSignalsHandler
    }
}

impl Actor for DefaultSignalsHandler {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = Arbiter::system_registry().get::<ProcessSignals>();
        let slf: Addr<Sync<_>> = ctx.address();
        addr.call(self, Subscribe(slf.subscriber()))
            .map(|_, _, _| ())
            .map_err(|_, _, _| ())
            .wait(ctx)
    }
}

/// Handle `SIGINT`, `SIGTERM`, `SIGQUIT` signals and send `SystemExit(0)`
/// message to `System` actor.
impl actix::Handler<Signal> for DefaultSignalsHandler {
    type Result = ();

    fn handle(&mut self, msg: Signal, _: &mut Self::Context) {
        match msg.0 {
            SignalType::Int => {
                info!("SIGINT received, exiting");
                Arbiter::system().send(actix::msgs::SystemExit(0));
            }
            SignalType::Hup => {
                info!("SIGHUP received, reloading");
            }
            SignalType::Term => {
                info!("SIGTERM received, stopping");
                Arbiter::system().send(actix::msgs::SystemExit(0));
            }
            SignalType::Quit => {
                info!("SIGQUIT received, exiting");
                Arbiter::system().send(actix::msgs::SystemExit(0));
            }
            _ => (),
        }
    }
}
