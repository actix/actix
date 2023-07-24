//! A message broker for the Actix actor framework.
//!
//! The `actix_broker` crate contains `SystemService` and `ArbiterService` Brokers that
//! keep track of active subscriptions to different `Messages`. Broker services are
//! automatically started when an actor uses functions from the `BrokerSubscribe` and
//! `BrokerIssue` traits to either subscribe to or issue a message.
//!
//! # Examples
//! ```no_run
//! # #[macro_use]
//! # extern crate actix;
//! # extern crate actix_broker;
//! use actix::prelude::*;
//! use actix_broker::{BrokerSubscribe, BrokerIssue, SystemBroker, ArbiterBroker, Broker};
//!
//! // Note: The message must implement 'Clone'
//! #[derive(Clone, Message)]
//! #[rtype(result = "()")]
//! struct MessageOne;
//!
//! #[derive(Clone, Message)]
//! #[rtype(result = "()")]
//! struct MessageTwo;
//!
//! #[derive(Clone, Message)]
//! #[rtype(result = "()")]
//! struct MessageThree;
//!
//! struct ActorOne;
//!
//! impl Actor for ActorOne {
//!     // Note: The actor context must be Asynchronous,
//!     // i.e. it cannot be 'SyncContext'
//!     type Context = Context<Self>;
//!
//!     fn started(&mut self,ctx: &mut Self::Context) {
//!         // Asynchronously subscribe to a message on the system (global) broker
//!         self.subscribe_system_async::<MessageOne>(ctx);
//!         // Asynchronously issue a message to any subscribers on the system (global) broker
//!         self.issue_system_async(MessageOne);
//!         // Synchronously subscribe to a message on the arbiter (local) broker
//!         self.subscribe_arbiter_sync::<MessageTwo>(ctx);
//!         // Synchronously issue a message to any subscribers on the arbiter (local) broker
//!         self.issue_arbiter_sync(MessageTwo, ctx);
//!     }
//! }
//!     
//!     // To subscribe to a messsage, the actor must handle it
//! impl Handler<MessageOne> for ActorOne {
//!     type Result = ();
//!
//!     fn handle(&mut self, msg: MessageOne, ctx: &mut Self::Context) {
//!         // An actor does not have to handle a message to just issue it
//!         self.issue_async::<SystemBroker, _>(MessageThree);
//!     }
//! }
//!
//! // Messages can also be sent from outside actors
//! fn my_function() {
//!     Broker::<SystemBroker>::issue_async(MessageOne);
//! }
//! #
//! # // Handler for MessageTwo...
//! # impl Handler<MessageTwo> for ActorOne {
//! #     type Result = ();
//!
//! #     fn handle(&mut self, msg: MessageTwo, ctx: &mut Self::Context) {
//! #     }
//! # }
//! # fn main() {}
//! ```

#![deny(rust_2018_idioms, nonstandard_style, future_incompatible)]
#![doc(html_logo_url = "https://actix.rs/img/logo.png")]
#![doc(html_favicon_url = "https://actix.rs/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod broker;
mod issue;
mod msgs;
mod subscribe;

pub use crate::{
    broker::{ArbiterBroker, Broker, SystemBroker},
    issue::BrokerIssue,
    msgs::BrokerMsg,
    subscribe::BrokerSubscribe,
};
