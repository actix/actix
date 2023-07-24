use std::any::TypeId;

use actix::prelude::*;

use crate::{
    broker::{ArbiterBroker, RegisteredBroker, SystemBroker},
    msgs::*,
};

/// The `BrokerIssue` provides functions to issue messages to subscribers.
///
/// This will not deliver the message to the actor that sent it.
pub trait BrokerIssue
where
    Self: Actor,
    <Self as Actor>::Context: AsyncContext<Self>,
{
    /// Asynchronously issue a message.
    /// This bypasses the mailbox capacity, and  will always queue the message.
    /// If the mailbox is closed, the message is silently dropped and the subscriber
    /// is detached from the broker.
    fn issue_async<T: RegisteredBroker, M: BrokerMsg>(&self, msg: M) {
        let broker = T::get_broker();
        broker.do_send(IssueAsync(msg, TypeId::of::<Self>()));
    }

    /// Synchronously issue a message.
    /// This also causes the broker to synchronously forward those messages on to any subscribers
    /// before handling any other messages.
    fn issue_sync<T: RegisteredBroker, M: BrokerMsg>(&self, msg: M, ctx: &mut Self::Context) {
        let broker = T::get_broker();
        broker
            .send(IssueSync(msg, TypeId::of::<Self>()))
            .into_actor(self)
            .map(|_, _, _| ())
            .wait(ctx);
    }

    /// Helper to asynchronously issue to an system broker
    /// This is the equivalent of `self.issue_async::<SystemBroker, M>(ctx);`
    fn issue_system_async<M: BrokerMsg>(&self, msg: M) {
        self.issue_async::<SystemBroker, M>(msg);
    }

    /// Helper to synchronously issue to an system broker
    /// This is the equivalent of `self.issue_sync::<SystemBroker, M>(ctx);`
    fn issue_system_sync<M: BrokerMsg>(&self, msg: M, ctx: &mut Self::Context) {
        self.issue_sync::<SystemBroker, M>(msg, ctx);
    }

    /// Helper to asynchronously issue to an arbiter-specific broker
    /// This is the equivalent of `self.issue_async::<ArbiterBroker, M>(ctx);`
    fn issue_arbiter_async<M: BrokerMsg>(&self, msg: M) {
        self.issue_async::<ArbiterBroker, M>(msg);
    }

    /// Helper to synchronously issue to an arbiter-specific broker
    /// This is the equivalent of `self.issue_sync::<ArbiterBroker, M>(ctx);`
    fn issue_arbiter_sync<M: BrokerMsg>(&self, msg: M, ctx: &mut Self::Context) {
        self.issue_sync::<ArbiterBroker, M>(msg, ctx);
    }
}

impl<A> BrokerIssue for A
where
    A: Actor,
    <A as Actor>::Context: AsyncContext<A>,
{
}
