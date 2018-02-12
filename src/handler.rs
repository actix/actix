use futures::Future;
use futures::sync::oneshot::Sender as SyncSender;
use futures::unsync::oneshot::Sender as UnsyncSender;

use arbiter::Arbiter;
use fut::{self, ActorFuture};
use actor::{Actor, AsyncContext};
use address::{Addr, Syn};
use context::Context;

/// Message handler
///
/// `Handler` implementation is a general way how to handle
/// incoming messages, streams, futures.
///
/// `M` is a message which can be handled by the actor.
#[allow(unused_variables)]
pub trait Handler<M> where Self: Actor, M: ResponseType {
    /// The type of value that this handle will return
    type Result: MessageResponse<Self, M>;

    /// Method is called for every message received by this Actor
    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> Self::Result;
}

/// Message response type
pub trait ResponseType {

    /// The type of value that this message will resolved with if it is successful.
    type Item: 'static;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error: 'static;
}

impl<I, E> ResponseType for Result<I, E> where I: ResponseType {
    type Item = <I as ResponseType>::Item;
    type Error = ();
}

/// A specialized [`Result`](https://doc.rust-lang.org/std/result/enum.Result.html)
/// for message result responses
pub type MessageResult<M: ResponseType> = Result<M::Item, M::Error>;

/// A specialized actor future for async message handler
pub type ResponseActFuture<A, M: ResponseType> =
    Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>;

/// A specialized future for async message handler
pub type ResponseFuture<M: ResponseType> = Box<Future<Item=M::Item, Error=M::Error>>;

/// Trait defines message response channel
pub trait ResponseChannel<M: ResponseType>: 'static {

    fn is_canceled(&self) -> bool;

    fn send(self, response: MessageResult<M>);
}

/// Trait define message response
pub trait MessageResponse<A: Actor, M: ResponseType> {
    fn handle<R: ResponseChannel<M>>(self, ctx: &mut A::Context, tx: Option<R>);
}

impl<M: ResponseType + 'static> ResponseChannel<M> for SyncSender<MessageResult<M>> {
    fn is_canceled(&self) -> bool {
        SyncSender::is_canceled(self)
    }

    fn send(self, response: MessageResult<M>) {
        let _ = SyncSender::send(self, response);
    }
}

impl<M: ResponseType + 'static> ResponseChannel<M> for UnsyncSender<MessageResult<M>> {
    fn is_canceled(&self) -> bool {
        UnsyncSender::is_canceled(self)
    }

    fn send(self, response: MessageResult<M>) {
        let _ = UnsyncSender::send(self, response);
    }
}

impl<M: ResponseType + 'static> ResponseChannel<M> for () {
    fn is_canceled(&self) -> bool {true}
    fn send(self, _: MessageResult<M>) {}
}

impl<A, M> MessageResponse<A, M> for () where A: Actor, M: ResponseType<Item=(), Error=()>
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(Ok(()));
        }
    }
}

impl<A, M> MessageResponse<A, M> for Result<M::Item, M::Error>
    where A: Actor, M: ResponseType,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

impl<A, M, B> MessageResponse<A, M> for Addr<Syn<B>>
    where A: Actor, M: ResponseType<Item=Addr<Syn<B>>, Error=()>,
          B: Actor<Context=Context<B>>
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(Ok(self));
        }
    }
}

impl<A, M> MessageResponse<A, M> for ResponseActFuture<A, M>
    where A: Actor, M: ResponseType, A::Context: AsyncContext<A>
{
    fn handle<R: ResponseChannel<M>>(self, ctx: &mut A::Context, tx: Option<R>) {
        ctx.spawn(
            self.then(move |res, _, _| {
                if let Some(tx) = tx {
                    tx.send(res);
                }
                fut::ok(())
            }));
    }
}

impl<A, M> MessageResponse<A, M> for ResponseFuture<M>
    where A: Actor, M: ResponseType, A::Context: AsyncContext<A>
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        Arbiter::handle().spawn(self.then(move |res| {
            tx.map(|tx| tx.send(res));
            Ok(())
        }));
    }
}


enum ResponseTypeItem<A, M> where A: Actor, M: ResponseType {
    Result(MessageResult<M>),
    Fut(Box<Future<Item=M::Item, Error=M::Error>>),
    AFut(Box<ActorFuture<Item=M::Item, Error=M::Error, Actor=A>>),
}

/// Helper type for representing different type of message responses
pub struct Response<A, M> where A: Actor, M: ResponseType {
    item: ResponseTypeItem<A, M>,
}

impl<A, M> Response<A, M> where A: Actor, M: ResponseType {

    /// Create async response
    pub fn fut<T>(fut: T) -> Self
        where T: Future<Item=M::Item, Error=M::Error> + 'static
    {
        Response {item: ResponseTypeItem::Fut(Box::new(fut))}
    }

    /// Create response
    pub fn reply(val: MessageResult<M>) -> Self {
        Response {item: ResponseTypeItem::Result(val)}
    }

    /// Create async response
    pub fn async_reply<T>(fut: T) -> Self
        where T: ActorFuture<Item=M::Item, Error=M::Error, Actor=A> + 'static
    {
        Response {item: ResponseTypeItem::AFut(Box::new(fut))}
    }
}

impl<A, M> MessageResponse<A, M> for Response<A, M>
    where A: Actor, M: ResponseType, A::Context: AsyncContext<A>
{
    fn handle<R: ResponseChannel<M>>(self, ctx: &mut A::Context, tx: Option<R>) {
        match self.item {
            ResponseTypeItem::Fut(fut) => {
                Arbiter::handle().spawn(fut.then(move |res| {
                    tx.map(|tx| tx.send(res));
                    Ok(())
                }));
            },
            ResponseTypeItem::AFut(fut) => {
                ctx.spawn(fut.then(move |res, _, _| {
                    tx.map(|tx| tx.send(res));
                    fut::ok(())
                }));
            },
            ResponseTypeItem::Result(res) => {
                tx.map(|tx| tx.send(res));
            },
        }
    }
}
