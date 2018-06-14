//! An actor implementation of Unix signal handling
//!
//! This module implements asynchronous signal handling for Actix. For each
//! signal `ProcessSignals` actor sends `Signal` message to all subscriber. To
//! subscriber, send `Subscribe` message to `ProcessSignals` actor.
//!
//! # Examples
//!
//! ```rust
//! # extern crate actix;
//! use actix::actors::signal;
//! use actix::prelude::*;
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
//!                 System::current().stop();
//!             }
//!             signal::SignalType::Hup => {
//!                 println!("SIGHUP received, reloading");
//!             }
//!             signal::SignalType::Term => {
//!                 println!("SIGTERM received, stopping");
//!                 System::current().stop();
//!             }
//!             signal::SignalType::Quit => {
//!                 println!("SIGQUIT received, exiting");
//!                 System::current().stop();
//!             }
//!             _ => (),
//!         }
//!     }
//! }
//!
//! fn main() {
//!     // initialize system
//!     System::run(|| {
//!         // Start signals handler
//!         let addr = Signals.start();
//!
//!         // send SIGTERM
//!         std::thread::spawn(move || {
//!             // emulate SIGNTERM
//!             addr.do_send(signal::Signal(signal::SignalType::Term));
//!         });
//!     });
//!
//!     std::process::exit(0);
//! }
//! ```
extern crate tokio_signal;

#[cfg(unix)]
use self::tokio_signal::unix;
use futures::{Future, Stream};
use libc;
use std;

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

impl Message for SignalType {
    type Result = ();
}

/// Process signal message
#[derive(Debug)]
pub struct Signal(pub SignalType);

impl Message for Signal {
    type Result = ();
}

/// An actor implementation of Unix signal handling
pub struct ProcessSignals {
    subscribers: Vec<Recipient<Signal>>,
}

impl Default for ProcessSignals {
    fn default() -> Self {
        ProcessSignals {
            subscribers: Vec::new(),
        }
    }
}

impl Actor for ProcessSignals {
    type Context = Context<Self>;
}

impl actix::Supervised for ProcessSignals {}

impl actix::SystemService for ProcessSignals {
    fn service_started(&mut self, ctx: &mut Self::Context) {
        // SIGINT
        tokio_signal::ctrl_c()
            .map_err(|_| ())
            .actfuture()
            .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>| {
                ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Int))
            })
            .spawn(ctx);

        #[cfg(unix)]
        {
            // SIGHUP
            unix::Signal::new(libc::SIGHUP)
                .actfuture()
                .drop_err()
                .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>| {
                    ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Hup))
                })
                .spawn(ctx);

            // SIGTERM
            unix::Signal::new(libc::SIGTERM)
                .map_err(|_| ())
                .actfuture()
                .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>| {
                    ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Term))
                })
                .spawn(ctx);

            // SIGQUIT
            unix::Signal::new(libc::SIGQUIT)
                .map_err(|_| ())
                .actfuture()
                .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>| {
                    ctx.add_message_stream(sig.map_err(|_| ()).map(|_| SignalType::Quit))
                })
                .spawn(ctx);

            // SIGCHLD
            unix::Signal::new(libc::SIGCHLD)
                .map_err(|_| ())
                .actfuture()
                .map(|sig, _: &mut Self, ctx: &mut actix::Context<Self>| {
                    ctx.add_message_stream(
                        sig.map_err(|_| ()).map(|_| SignalType::Child),
                    )
                })
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
            if subscr.do_send(Signal(sig)).is_ok() {
                self.subscribers.push(subscr);
            }
        }
    }
}

/// Subscribe to process signals.
pub struct Subscribe(pub Recipient<Signal>);

impl Message for Subscribe {
    type Result = ();
}

/// Add subscriber for signals
impl actix::Handler<Subscribe> for ProcessSignals {
    type Result = ();

    fn handle(&mut self, msg: Subscribe, _: &mut Self::Context) {
        self.subscribers.push(msg.0);
    }
}

/// Default signals handler. This actor sends `SystemExit` message to `System`
/// actor for each of `SIGINT`, `SIGTERM`, `SIGQUIT` signals.
pub struct DefaultSignalsHandler;

impl Default for DefaultSignalsHandler {
    fn default() -> Self {
        DefaultSignalsHandler
    }
}

impl Actor for DefaultSignalsHandler {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = System::current().registry().get::<ProcessSignals>();
        let slf = ctx.address();
        addr.send(Subscribe(slf.recipient()))
            .map(|_| ())
            .map_err(|_| ())
            .into_actor(self)
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
                System::current().stop();
            }
            SignalType::Hup => {
                info!("SIGHUP received, reloading");
            }
            SignalType::Term => {
                info!("SIGTERM received, stopping");
                System::current().stop();
            }
            SignalType::Quit => {
                info!("SIGQUIT received, exiting");
                System::current().stop();
            }
            _ => (),
        }
    }
}
