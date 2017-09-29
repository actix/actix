use std::cell::Cell;
use futures::{Future, Poll};
use futures::unsync::oneshot::{channel, Canceled, Receiver};
use futures::sync::oneshot::{channel as sync_channel,
                             Canceled as SyncCanceled, Receiver as SyncReceiver};

use actor::{Actor, MessageHandler, MessageResponse};
use context::{Context, ContextProtocol};
use envelope::Envelope;
use message::Request;
use queue::{sync, unsync};


#[doc(hidden)]
pub trait ActorAddress<A, T> where A: Actor {

    fn get(ctx: &mut Context<A>) -> T;
}

pub trait Subscriber<M: 'static> {

    /// Buffered send
    fn send(&self, msg: M);

    /// Unbuffered send
    fn unbuffered_send(&self, msg: M) -> Result<(), M>;
}

pub trait AsyncSubscriber<M> {

    type Future: Future;

    /// Send message, wait response asynchronously
    fn call(&self, msg: M) -> Self::Future;

    /// Send message, wait response asynchronously
    fn unbuffered_call(&self, msg: M) -> Result<Self::Future, M>;

}

/// Address of the actor `A`.
/// Actor has to run in the same thread as owner of the address.
pub struct Address<A> where A: Actor {
    tx: unsync::UnboundedSender<ContextProtocol<A>>
}

impl<A> Clone for Address<A> where A: Actor {
    fn clone(&self) -> Self {
        Address{tx: self.tx.clone() }
    }
}

impl<A> Address<A> where A: Actor {

    pub(crate) fn new(sender: unsync::UnboundedSender<ContextProtocol<A>>) -> Address<A> {
        Address{tx: sender}
    }

    /// Send message `M` to actor `A`.
    pub fn send<M: 'static>(&self, msg: M) where A: MessageHandler<M>
    {
        let _ = self.tx.unbounded_send(
            ContextProtocol::Envelope(Envelope::local(msg, None)));
    }

    /// Send message to actor `A` and asyncronously wait for response.
    pub fn call<B: Actor, M>(&self, msg: M) -> Request<A, B, M>
        where A: MessageHandler<M>,
              M: 'static
    {
        let (tx, rx) = channel();
        let _ = self.tx.unbounded_send(
            ContextProtocol::Envelope(Envelope::local(msg, Some(tx))));

        Request::local(rx)
    }

    /// Send message to actor `A` and asyncronously wait for response.
    pub fn call_fut<M>(&self, msg: M) -> Receiver<Result<A::Item, A::Error>>
        where A: MessageHandler<M>,
              M: 'static
    {
        let (tx, rx) = channel();
        let _ = self.tx.unbounded_send(
            ContextProtocol::Envelope(Envelope::local(msg, Some(tx))));

        rx
    }

    /// Upgrade address to SyncAddress.
    pub fn upgrade(&self) -> Receiver<SyncAddress<A>> {
        let (tx, rx) = channel();
        let _ = self.tx.unbounded_send(
            ContextProtocol::SyncAddress(tx));

        rx
    }

    /// Get `Subscriber` for specific message type
    pub fn subscriber<M: 'static>(&self) -> Box<Subscriber<M>>
        where A: MessageHandler<M>
    {
        Box::new(self.clone())
    }
}

impl<A, M: 'static> Subscriber<M> for Address<A>
    where A: Actor + MessageHandler<M>
{

    fn send(&self, msg: M) {
        self.send(msg)
    }

    fn unbuffered_send(&self, msg: M) -> Result<(), M> {
        self.send(msg);
        Ok(())
    }
}

impl<A, M: 'static> AsyncSubscriber<M> for Address<A>
    where A: Actor + MessageHandler<M>
{
    type Future = CallResult<A::Item, A::Error>;

    fn call(&self, msg: M) -> Self::Future
    {
        let (tx, rx) = channel();
        let _ = self.tx.unbounded_send(
            ContextProtocol::Envelope(Envelope::local(msg, Some(tx))));

        CallResult::new(rx)
    }

    fn unbuffered_call(&self, msg: M) -> Result<Self::Future, M>
    {
        let (tx, rx) = channel();
        let _ = self.tx.unbounded_send(
            ContextProtocol::Envelope(Envelope::local(msg, Some(tx))));

        Ok(CallResult::new(rx))
    }
}

impl<A> ActorAddress<A, Address<A>> for A where A: Actor {

    fn get(ctx: &mut Context<A>) -> Address<A> {
        ctx.address_cell().unsync_address()
    }
}

impl<A> ActorAddress<A, (Address<A>, SyncAddress<A>)> for A where A: Actor {

    fn get(ctx: &mut Context<A>) -> (Address<A>, SyncAddress<A>) {
        (ctx.address_cell().unsync_address(), ctx.address_cell().sync_address())
    }
}

impl<A> ActorAddress<A, ()> for A where A: Actor {

    fn get(_: &mut Context<A>) -> () {
        ()
    }
}

#[must_use = "future do nothing unless polled"]
pub struct CallResult<I, E>
{
    rx: Receiver<Result<I, E>>,
}

impl<I, E> CallResult<I, E>
{
    pub(crate) fn new(rx: Receiver<Result<I, E>>) -> CallResult<I, E> {
        CallResult{rx: rx}
    }
}

impl<I, E> Future for CallResult<I, E>
{
    type Item = Result<I, E>;
    type Error = Canceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error>
    {
        self.rx.poll()
    }
}

/// Address of the actor `A`. Actor can run in differend thread.
pub struct SyncAddress<A> where A: Actor {
    tx: sync::UnboundedSender<Envelope<A>>,
    closed: Cell<bool>,
}

impl<A> Clone for SyncAddress<A> where A: Actor {
    fn clone(&self) -> Self {
    SyncAddress{tx: self.tx.clone(), closed: self.closed.clone()}
}
}

impl<A> ActorAddress<A, SyncAddress<A>> for A where A: Actor {

    fn get(ctx: &mut Context<A>) -> SyncAddress<A> {
        ctx.address_cell().sync_address()
    }
}

impl<A> SyncAddress<A> where A: Actor {

    pub(crate) fn new(sender: sync::UnboundedSender<Envelope<A>>) -> SyncAddress<A> {
        SyncAddress{tx: sender, closed: Cell::new(false)}
    }

    /// Indicates if address is closed on other side.
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }

    /// Send message `M` to actor `A`. Message cold be sent to actor running in
    /// different thread.
    pub fn send<M: 'static + Send>(&self, msg: M)
        where A: MessageHandler<M> + MessageResponse<M>,
              A::Item: Send,
              A::Error: Send,
    {
        if self.tx.unbounded_send(Envelope::remote(msg, None)).is_err()
        {
            self.closed.set(true)
        }
    }

    /// Send message to actor `A` and asyncronously wait for response.
    pub fn call<B: Actor, M: 'static + Send>(&self, msg: M) -> Request<A, B, M>
        where A: MessageHandler<M>,
              A::Item: Send,
              A::Error: Send,
    {
        let (tx, rx) = sync_channel();
        if self.tx.unbounded_send(Envelope::remote(msg, Some(tx))).is_err()
        {
            self.closed.set(true)
        }

        Request::remote(rx)
    }

    /// Send message to actor `A` and asyncronously wait for response.
    pub fn call_fut<M>(&self, msg: M) -> SyncReceiver<Result<A::Item, A::Error>>
        where A: MessageHandler<M>,
              M: Send + 'static,
              A::Item: Send,
              A::Error: Send,
    {
        let (tx, rx) = sync_channel();
        if self.tx.unbounded_send(Envelope::remote(msg, Some(tx))).is_err()
        {
            self.closed.set(true)
        }

        rx
    }

    /// Get `Subscriber` for specific message type
    pub fn subscriber<M: 'static + Send>(&self) -> Box<Subscriber<M> + Send>
        where A: MessageHandler<M>,
              A::Item: Send,
              A::Error: Send,
    {
        Box::new(self.clone())
    }

    pub fn async_subscriber<M>(
        &self) -> Box<AsyncSubscriber<M, Future=SyncCallResult<A::Item, A::Error>>>
        where A: MessageHandler<M>,
              A::Item: Send,
              A::Error: Send,
              M: 'static + Send,
    {
        Box::new(self.clone())
    }
}

impl<A, M> Subscriber<M> for SyncAddress<A>
    where M: 'static + Send,
          A::Item: Send,
          A::Error: Send,
          A: Actor + MessageHandler<M>
{
    fn send(&self, msg: M) {
        self.send(msg)
    }

    fn unbuffered_send(&self, msg: M) -> Result<(), M> {
        self.send(msg);
        Ok(())
    }
}

impl<A, M> AsyncSubscriber<M> for SyncAddress<A>
    where M: 'static + Send,
          A: Actor + MessageHandler<M>,
          A::Item: Send,
          A::Error: Send,
{
    type Future = SyncCallResult<A::Item, A::Error>;

    fn call(&self, msg: M) -> Self::Future
    {
        let (tx, rx) = sync_channel();
        if self.tx.unbounded_send(Envelope::remote(msg, Some(tx))).is_err()
        {
            self.closed.set(true)
        }

        SyncCallResult::new(rx)
    }

    fn unbuffered_call(&self, msg: M) -> Result<Self::Future, M>
    {
        Ok(AsyncSubscriber::call(self, msg))
    }
}

#[must_use = "future do nothing unless polled"]
pub struct SyncCallResult<I, E>
{
    rx: SyncReceiver<Result<I, E>>,
}

impl<I, E> SyncCallResult<I, E>
{
    fn new(rx: SyncReceiver<Result<I, E>>) -> SyncCallResult<I, E> {
        SyncCallResult{rx: rx}
    }
}

impl<I, E> Future for SyncCallResult<I, E>
{
    type Item = Result<I, E>;
    type Error = SyncCanceled;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.rx.poll()
    }
}
