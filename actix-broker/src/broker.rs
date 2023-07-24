use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::BuildHasherDefault,
    marker::PhantomData,
};

use actix::prelude::*;
use ahash::AHasher;
use log::trace;

use crate::msgs::*;

type TypeMap<A> = HashMap<TypeId, A, BuildHasherDefault<AHasher>>;

#[derive(Default)]
pub struct Broker<T> {
    sub_map: TypeMap<Vec<(TypeId, Box<dyn Any>)>>,
    msg_map: TypeMap<Box<dyn Any>>,
    _t: PhantomData<T>,
}

#[derive(Default)]
pub struct SystemBroker;

#[derive(Default)]
pub struct ArbiterBroker;

/// The system service actor that keeps track of subscriptions and routes messages to them.
impl Broker<SystemBroker> {
    /// Send messages asynchronously via the broker. It can be called from with
    /// actors with a `SyncContext`, or where you don't have access to `self`. e.g. From within
    /// a `HttpHandler` from `actix-web`.
    pub fn issue_async<M: BrokerMsg>(msg: M) {
        let broker = Self::from_registry();
        broker.do_send(IssueAsync(msg, TypeId::of::<Self>()));
    }
}

/// The system service actor that keeps track of subscriptions and routes messages to them.
impl Broker<ArbiterBroker> {
    /// Send messages asynchronously via the broker. It can be called from with
    /// actors with a `SyncContext`, or where you don't have access to `self`. e.g. From within
    /// a `HttpHandler` from `actix-web`.
    pub fn issue_async<M: BrokerMsg>(msg: M) {
        let broker = Self::from_registry();
        broker.do_send(IssueAsync(msg, TypeId::of::<Self>()));
    }
}

/// The system service actor that keeps track of subscriptions and routes messages to them.
impl<T> Broker<T> {
    fn take_subs<M: BrokerMsg>(&mut self) -> Option<Vec<(TypeId, Recipient<M>)>> {
        let id = TypeId::of::<M>();
        let subs = self.sub_map.get_mut(&id)?;
        trace!("Broker: Found subscription list for {:?}.", id);
        let subs = subs
            .drain(..)
            .filter_map(|(id, s)| {
                if let Ok(rec) = s.downcast::<Recipient<M>>() {
                    Some((id, rec))
                } else {
                    None
                }
            })
            .map(|(id, s)| (id, *s))
            .collect();
        Some(subs)
    }

    fn add_sub<M: BrokerMsg>(&mut self, sub: Recipient<M>, id: TypeId) {
        let msg_id = TypeId::of::<M>();
        let boxed = Box::new(sub);
        if let Some(subs) = self.sub_map.get_mut(&msg_id) {
            trace!("Broker: Adding to {:?} subscription list.", msg_id);
            subs.push((id, boxed));
            return;
        }

        trace!("Broker: Creating {:?} subscription list.", msg_id);
        self.sub_map.insert(msg_id, vec![(id, boxed)]);
    }

    fn get_previous_msg<M: BrokerMsg>(&self) -> Option<M> {
        let id = TypeId::of::<M>();
        let msg = self.msg_map.get(&id)?;
        trace!("Broker: Previous message found for {:?}", id);
        let msg = msg.downcast_ref::<M>()?;
        Some(msg.clone())
    }

    fn set_msg<M: BrokerMsg>(&mut self, msg: M) {
        let id = TypeId::of::<M>();
        let boxed = Box::new(msg);
        if let Some(pm) = self.msg_map.get_mut(&id) {
            trace!("Broker: Setting new message value for {:?}", id);
            *pm = boxed;
            return;
        }

        trace!("Broker: Adding first message value for {:?}", id);
        self.msg_map.insert(id, boxed);
    }
}

impl<T: 'static + Unpin, M: BrokerMsg> Handler<SubscribeAsync<M>> for Broker<T> {
    type Result = ();

    fn handle(&mut self, msg: SubscribeAsync<M>, _ctx: &mut Context<Self>) {
        trace!("Broker: Received SubscribeAsync");
        self.add_sub::<M>(msg.0, msg.1);
    }
}

impl<T: 'static + Unpin, M: BrokerMsg> Handler<SubscribeSync<M>> for Broker<T> {
    type Result = Option<M>;

    fn handle(&mut self, msg: SubscribeSync<M>, _ctx: &mut Context<Self>) -> Self::Result {
        trace!("Broker: Received SubscribeSync");
        self.add_sub::<M>(msg.0, msg.1);
        self.get_previous_msg::<M>()
    }
}

impl<T: 'static + Unpin, M: BrokerMsg> Handler<IssueAsync<M>> for Broker<T> {
    type Result = ();

    fn handle(&mut self, msg: IssueAsync<M>, _ctx: &mut Context<Self>) {
        trace!("Broker: Received IssueAsync");
        if let Some(mut subs) = self.take_subs::<M>() {
            subs.drain(..).for_each(|(id, s)| {
                if id == msg.1 {
                    self.add_sub::<M>(s, id);
                } else {
                    match s.try_send(msg.0.clone()) {
                        Ok(_) => self.add_sub::<M>(s, id),
                        Err(SendError::Full(_)) => {
                            // Ensure that that the message is delivered even if the mailbox is full.
                            // We do a try first to remove receiver that have closed their mailbox.
                            s.do_send(msg.0.clone());
                            self.add_sub::<M>(s, id);
                        }
                        Err(_) => (),
                    }
                }
            });
        }
        self.set_msg::<M>(msg.0);
    }
}

impl<T: 'static + Unpin, M: BrokerMsg> Handler<IssueSync<M>> for Broker<T> {
    type Result = ();

    fn handle(&mut self, msg: IssueSync<M>, ctx: &mut Context<Self>) {
        trace!("Broker: Received IssueSync");
        if let Some(mut subs) = self.take_subs::<M>() {
            subs.drain(..).for_each(|(id, s)| {
                if id == msg.1 {
                    self.add_sub::<M>(s, id);
                } else {
                    s.send(msg.0.clone())
                        .into_actor(self)
                        .map(move |_, act, _| act.add_sub::<M>(s, id))
                        .wait(ctx);
                }
            });
        }
        self.set_msg::<M>(msg.0);
    }
}

impl<T: 'static + Unpin> Actor for Broker<T> {
    type Context = Context<Self>;
}

impl SystemService for Broker<SystemBroker> {}
impl Supervised for Broker<SystemBroker> {}

impl ArbiterService for Broker<ArbiterBroker> {}
impl Supervised for Broker<ArbiterBroker> {}

pub trait RegisteredBroker: 'static + Unpin
where
    Self: std::marker::Sized,
{
    fn get_broker() -> Addr<Broker<Self>>;
}

impl RegisteredBroker for SystemBroker {
    fn get_broker() -> Addr<Broker<Self>> {
        Broker::<SystemBroker>::from_registry()
    }
}

impl RegisteredBroker for ArbiterBroker {
    fn get_broker() -> Addr<Broker<Self>> {
        Broker::<ArbiterBroker>::from_registry()
    }
}
