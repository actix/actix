//! messages.
use std::any::TypeId;

use actix::{dev::ToEnvelope, prelude::*};

use crate::{
    broker::{ArbiterBroker, RegisteredBroker, SystemBroker},
    msgs::*,
};

/// The `BrokerSubscribe` trait has functions to register an actor's interest in different
/// messages.
pub trait BrokerSubscribe
where
    Self: Actor,
    <Self as Actor>::Context: AsyncContext<Self>,
{
    /// Asynchronously subscribe to a message.
    fn subscribe_async<T: RegisteredBroker, M: BrokerMsg>(&self, ctx: &mut Self::Context)
    where
        Self: Handler<M>,
        <Self as Actor>::Context: ToEnvelope<Self, M>,
    {
        let broker = T::get_broker();
        let recipient = ctx.address().recipient::<M>();
        broker.do_send(SubscribeAsync(recipient, TypeId::of::<Self>()));
    }

    /// Synchronously subscribe to a message.
    /// This actor will do nothing else until its interest is registered.
    /// If messages of that type have been sent to the broker previously, a copy of the latest
    /// message is sent to the calling actor after it has subscribed.
    fn subscribe_sync<T: RegisteredBroker, M: BrokerMsg>(&self, ctx: &mut Self::Context)
    where
        Self: Handler<M>,
        <Self as Actor>::Context: ToEnvelope<Self, M>,
    {
        let broker = T::get_broker();
        let recipient = ctx.address().recipient::<M>();

        broker
            .send(SubscribeSync(recipient, TypeId::of::<Self>()))
            .into_actor(self)
            .map(move |m, _, ctx| {
                if let Ok(Some(msg)) = m {
                    ctx.notify(msg);
                }
            })
            .wait(ctx);
    }

    /// Helper to asynchronously subscribe to a system broker
    /// This is the equivalent of `self.subscribe_async::<SystemBroker, M>(ctx);`
    fn subscribe_system_async<M: BrokerMsg>(&self, ctx: &mut Self::Context)
    where
        Self: Handler<M>,
        <Self as Actor>::Context: ToEnvelope<Self, M>,
    {
        self.subscribe_async::<SystemBroker, M>(ctx);
    }

    /// Helper to synchronously subscribe to a system broker
    /// This is the equivalent of `self.subscribe_sync::<SystemBroker, M>(ctx);
    fn subscribe_system_sync<M: BrokerMsg>(&self, ctx: &mut Self::Context)
    where
        Self: Handler<M>,
        <Self as Actor>::Context: ToEnvelope<Self, M>,
    {
        self.subscribe_sync::<SystemBroker, M>(ctx);
    }

    /// Helper to asynchronously subscribe to an arbiter-specific broker
    /// This is the equivalent of `self.subscribe_async::<ArbiterBroker, M>(ctx);`
    fn subscribe_arbiter_async<M: BrokerMsg>(&self, ctx: &mut Self::Context)
    where
        Self: Handler<M>,
        <Self as Actor>::Context: ToEnvelope<Self, M>,
    {
        self.subscribe_async::<ArbiterBroker, M>(ctx);
    }

    /// Helper to synchronously subscribe to an arbiter-specific broker
    /// This is the equivalent of `self.subscribe_sync::<ArbiterBroker, M>(ctx);
    fn subscribe_arbiter_sync<M: BrokerMsg>(&self, ctx: &mut Self::Context)
    where
        Self: Handler<M>,
        <Self as Actor>::Context: ToEnvelope<Self, M>,
    {
        self.subscribe_sync::<ArbiterBroker, M>(ctx);
    }
}

impl<A> BrokerSubscribe for A
where
    A: Actor,
    <A as Actor>::Context: AsyncContext<A>,
{
}
