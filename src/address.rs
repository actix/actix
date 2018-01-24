use std::mem;
use std::cell::Cell;
use futures::AsyncSink;
use futures::unsync::oneshot::channel;
use futures::sync::oneshot::{channel as sync_channel, Receiver as SyncReceiver};

use actor::{Actor, AsyncContext};
use handler::{Handler, ResponseType};
use context::{AsyncContextApi, ContextProtocol};
use envelope::{Envelope, ToEnvelope};
use message::{Request, LocalRequest, LocalFutRequest, UpgradeAddress};
use queue::{sync, unsync, MessageOption, Either};


pub enum SendError<T> {
    NotReady(T),
    Closed(T),
}

/// Trait give access to actor's address
pub trait ActorAddress<A, T> where A: Actor {
    /// Returns actor's address for specific context
    fn get(ctx: &mut A::Context) -> T;
}

impl<A> ActorAddress<A, LocalAddress<A>> for A
    where A: Actor, A::Context: AsyncContext<A> + AsyncContextApi<A>
{
    fn get(ctx: &mut A::Context) -> LocalAddress<A> {
        ctx.local_address()
    }
}

impl<A> ActorAddress<A, Address<A>> for A
    where A: Actor, A::Context: AsyncContext<A> + AsyncContextApi<A>
{
    fn get(ctx: &mut A::Context) -> Address<A> {
        ctx.remote_address()
    }
}

impl<A> ActorAddress<A, (LocalAddress<A>, Address<A>)> for A
    where A: Actor, A::Context: AsyncContext<A> + AsyncContextApi<A>
{
    fn get(ctx: &mut A::Context) -> (LocalAddress<A>, Address<A>) {
        (ctx.local_address(), ctx.remote_address())
    }
}

impl<A> ActorAddress<A, ()> for A where A: Actor {
    fn get(_: &mut A::Context) -> () {
        ()
    }
}

/// Subscriber trait describes ability of actor to receive one specific message
///
/// You can get subscriber with `Address::subscriber()` or
/// `Address::subscriber()` methods. Both methods returns boxed trait object.
///
/// It is possible to use `Clone::clone()` method to get cloned subscriber.
pub trait Subscriber<M: 'static> {
    /// Send buffered message
    fn send(&self, msg: M) -> Result<(), SendError<M>>;

    #[doc(hidden)]
    /// Create boxed clone of the current subscriber
    fn boxed(&self) -> Box<Subscriber<M>>;
}

/// Convenience impl to allow boxed Subscriber objects to be cloned using `Clone.clone()`.
impl<M: 'static> Clone for Box<Subscriber<M>> {
    fn clone(&self) -> Box<Subscriber<M>> {
        self.boxed()
    }
}

/// Convenience impl to allow boxed Subscriber objects to be cloned using `Clone.clone()`.
impl<M: 'static> Clone for Box<Subscriber<M> + Send> {
    fn clone(&self) -> Box<Subscriber<M> + Send> {
        // simplify ergonomics of `+Send` subscriber, otherwise
        // it would require new trait with custom `.boxed()` method.
        unsafe { mem::transmute(self.boxed()) }
    }
}

/// Local address of the actor
///
/// Actor has to run in the same thread as owner of the address.
pub struct LocalAddress<A> where A: Actor, A::Context: AsyncContext<A> {
    tx: unsync::Sender<ContextProtocol<A>>
}

impl<A> Clone for LocalAddress<A> where A: Actor, A::Context: AsyncContext<A> {
    fn clone(&self) -> Self {
        LocalAddress{tx: self.tx.clone()}
    }
}

impl<A> LocalAddress<A> where A: Actor, A::Context: AsyncContext<A> {

    pub(crate) fn new(sender: unsync::Sender<ContextProtocol<A>>) -> LocalAddress<A> {
        LocalAddress{tx: sender}
    }

    /// Indicates if address is still connected to the actor.
    pub fn connected(&self) -> bool {
        self.tx.connected()
    }

    /// Send message `M` to actor `A`. Communication channel to the actor is bounded.
    pub fn send<M>(&self, msg: M) -> Result<(), SendError<M>>
        where A: Handler<M>, M: ResponseType + 'static
    {
        let res = self.tx.send(move |opt| match opt {
            MessageOption::Message => Either::Message((msg, ())),
            MessageOption::Envelope => Either::Envelope(
                ContextProtocol::Envelope(Envelope::local(msg, None, false)))
        });

        match res {
            Ok(AsyncSink::Ready) => Ok(()),
            Ok(AsyncSink::NotReady(msg)) => Err(SendError::Closed(msg.0)),
            Err(msg) => Err(SendError::NotReady(msg.0)),
        }
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `Request` object get dropped, message cancels.
    pub fn call<B, M>(&self, _: &B, msg: M) -> LocalRequest<A, B, M>
        where A: Handler<M>, M: ResponseType + 'static, B: Actor, B::Context: AsyncContext<B>
    {
        let (tx, rx) = channel();
        let res = self.tx.send(move |opt| match opt {
            MessageOption::Message => Either::Message((msg, tx)),
            MessageOption::Envelope => Either::Envelope(
                ContextProtocol::Envelope(Envelope::local(msg, Some(tx), true)))
        });

        match res {
            Ok(AsyncSink::Ready) => LocalRequest::new(Some(rx), None),
            Ok(AsyncSink::NotReady(msg)) =>
                LocalRequest::new(Some(rx), Some((self.tx.clone(), msg.1, msg.0))),
            Err(_) => LocalRequest::new(None, None),
        }
    }

    /// Send message to the actor `A` and asynchronously wait for response.
    ///
    /// Communication channel to the actor is bounded.
    ///
    /// if returned `Receiver` object get dropped, message cancels.
    pub fn call_fut<M>(&self, msg: M) -> LocalFutRequest<A, M>
        where A: Handler<M>, M: ResponseType + 'static
    {
        let (tx, rx) = channel();
        let res = self.tx.send(move |opt| match opt {
            MessageOption::Message => Either::Message((msg, tx)),
            MessageOption::Envelope => Either::Envelope(
                ContextProtocol::Envelope(Envelope::local(msg, Some(tx), true)))
        });

        match res {
            Ok(AsyncSink::Ready) =>
                LocalFutRequest::new(Some(rx), None),
            Ok(AsyncSink::NotReady(msg)) =>
                LocalFutRequest::new(Some(rx), Some((self.tx.clone(), msg.1, msg.0))),
            Err(_) =>
                LocalFutRequest::new(None, None),
        }
    }

    /// Upgrade address to remote Address.
    pub fn upgrade(&self) -> UpgradeAddress<A> {
        UpgradeAddress::new(self.tx.clone())
    }

    /// Get `Subscriber` for specific message type
    pub fn subscriber<M>(&self) -> Box<Subscriber<M>>
        where A: Handler<M>, M: ResponseType + 'static
    {
        Box::new(Clone::clone(self))
    }
}

impl<A, M> Subscriber<M> for LocalAddress<A>
    where A: Actor + Handler<M>, A::Context: AsyncContext<A>,
          M: ResponseType + 'static
{
    fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.send(msg)
    }

    #[doc(hidden)]
    fn boxed(&self) -> Box<Subscriber<M>> {
        Box::new(self.clone())
    }
}

/// `Send` address of the actor. Actor can run in different thread
pub struct Address<A> where A: Actor {
    tx: sync::UnboundedSender<Envelope<A>>,
    closed: Cell<bool>,
}

unsafe impl<A> Send for Address<A> where A: Actor {}
unsafe impl<A> Sync for Address<A> where A: Actor {}

impl<A> Clone for Address<A> where A: Actor {
    fn clone(&self) -> Self {
        Address{tx: self.tx.clone(), closed: self.closed.clone()}
    }
}

impl<A> Address<A> where A: Actor {

    pub(crate) fn new(sender: sync::UnboundedSender<Envelope<A>>) -> Address<A> {
        Address{tx: sender, closed: Cell::new(false)}
    }

    /// Indicates if address is still connected to the actor.
    pub fn connected(&self) -> bool {
        !self.closed.get()
    }

    /// Send message `M` to actor `A`. Message cold be sent to actor running in
    /// different thread.
    pub fn send<M>(&self, msg: M)
        where A: Handler<M>, <A as Actor>::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
    {
        if self.tx.unbounded_send(
            <<A as Actor>::Context as ToEnvelope<A>>::pack(msg, None, false)).is_err()
        {
            self.closed.set(true)
        }
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// if returned `Request` object get dropped, message cancels.
    pub fn call<B: Actor, M>(&self, _: &B, msg: M) -> Request<A, B, M>
        where A: Handler<M>,
              A::Context: ToEnvelope<A>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
              B::Context: AsyncContext<B>,
    {
        let (tx, rx) = sync_channel();
        if self.tx.unbounded_send(
            <A::Context as ToEnvelope<A>>::pack(msg, Some(tx), true)).is_err()
        {
            self.closed.set(true)
        }
        Request::new(rx)
    }

    /// Send message to actor `A` and asynchronously wait for response.
    ///
    /// if returned `Receiver` object get dropped, message cancels.
    pub fn call_fut<M>(&self, msg: M) -> SyncReceiver<Result<M::Item, M::Error>>
        where A: Handler<M>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
             <A as Actor>::Context: ToEnvelope<A>,
    {
        let (tx, rx) = sync_channel();
        if self.tx.unbounded_send(
            <A::Context as ToEnvelope<A>>::pack(msg, Some(tx), true)).is_err()
        {
            self.closed.set(true)
        }

        rx
    }

    /// Get `Subscriber` for specific message type
    pub fn subscriber<M: 'static + Send>(&self) -> Box<Subscriber<M> + Send>
        where A: Handler<M>,
              M: ResponseType + Send + 'static,
              M::Item: Send, M::Error: Send,
             <A as Actor>::Context: ToEnvelope<A>,
    {
        Box::new(self.clone())
    }
}

impl<A, M> Subscriber<M> for Address<A>
    where A: Actor + Handler<M>,
          <A as Actor>::Context: ToEnvelope<A>,
          M: ResponseType + Send + 'static,
          M::Item: Send, M::Error: Send,
{
    fn send(&self, msg: M) -> Result<(), SendError<M>> {
        if self.connected() {
            self.send(msg);
            Ok(())
        } else {
            Err(SendError::Closed(msg))
        }
    }

    #[doc(hidden)]
    fn boxed(&self) -> Box<Subscriber<M>> {
        Box::new(self.clone())
    }
}
