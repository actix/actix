use std::any::TypeId;

use actix::prelude::*;

pub trait BrokerMsg: Message<Result = ()> + Send + Clone + 'static {}

impl<M> BrokerMsg for M where M: Message<Result = ()> + Send + Clone + 'static {}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SubscribeAsync<M: BrokerMsg>(pub Recipient<M>, pub TypeId);

pub struct SubscribeSync<M: BrokerMsg>(pub Recipient<M>, pub TypeId);

impl<M: BrokerMsg> Message for SubscribeSync<M> {
    type Result = Option<M>;
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct IssueAsync<M: BrokerMsg>(pub M, pub TypeId);

#[derive(Message)]
#[rtype(result = "()")]
pub struct IssueSync<M: BrokerMsg>(pub M, pub TypeId);
