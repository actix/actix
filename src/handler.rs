use actor::Actor;
use fut::ActorFuture;
use message::Response;
use context::Context;
use address::{Address, SyncAddress};

/// Message response type
pub trait ResponseType {

    /// The type of value that this message will resolved with if it is successful.
    type Item;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error;
}

impl<I, E> ResponseType for Result<I, E> where I: ResponseType {
    type Item = <I as ResponseType>::Item;
    type Error = ();
}

pub type MessageResult<M: ResponseType> = Result<M::Item, M::Error>;

pub trait IntoResponse<A: Actor, M: ResponseType> {
    fn into_response(self) -> Response<A, M>;
}

impl<A, M> IntoResponse<A, M> for ()
    where A: Actor + Handler<M>, M: ResponseType<Item=(), Error=()>,
{
    fn into_response(self) -> Response<A, M> {
        Response::reply(Ok(()))
    }
}

impl<A, M> IntoResponse<A, M> for Response<A, M>
    where A: Actor + Handler<M>, M: ResponseType,
{
    fn into_response(self) -> Response<A, M> {
        self
    }
}

impl<A, M> IntoResponse<A, M> for Result<M::Item, M::Error>
    where A: Actor + Handler<M>, M: ResponseType,
{
    fn into_response(self) -> Response<A, M> {
        self.into()
    }
}

impl<A, M, B> IntoResponse<A, M> for SyncAddress<B>
    where A: Actor + Handler<M>, M: ResponseType<Item=SyncAddress<B>, Error=()>,
          B: Actor<Context=Context<B>>
{
    fn into_response(self) -> Response<A, M> {
        Response::reply(Ok(self))
    }
}

pub type ResponseFuture<A: Actor, M: ResponseType> =
    Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>;

impl<A, M> IntoResponse<A, M> for Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>
    where A: Actor + Handler<M>, M: ResponseType
{
    fn into_response(self) -> Response<A, M> {
        self.into()
    }
}

/// Message handler
///
/// `Handler` implementation is a general way how to handle
/// incoming messages, streams, futures.
///
/// `M` is a message which can be handled by the actor.
#[allow(unused_variables)]
pub trait Handler<M> where Self: Actor, M: ResponseType
{
    type Result: IntoResponse<Self, M>;

    /// Method is called for every message received by this Actor
    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> Self::Result;
}
