use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{self, Poll};
use std::time::Duration;

use futures_util::{ready, stream::Stream};
use pin_project::pin_project;

use crate::actor::{Actor, ActorContext, AsyncContext};
use crate::clock::Delay;
use crate::fut::ActorFuture;
use crate::handler::{Handler, Message, MessageResponse};

pub(crate) struct ActorWaitItem<A: Actor>(
    Pin<Box<dyn ActorFuture<Output = (), Actor = A>>>,
);

impl<A> ActorWaitItem<A>
where
    A: Actor,
    A::Context: ActorContext + AsyncContext<A>,
{
    #[inline]
    pub fn new<F>(fut: F) -> Self
    where
        F: ActorFuture<Output = (), Actor = A> + 'static,
    {
        ActorWaitItem(Box::pin(fut))
    }

    pub fn poll(
        mut self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<()> {
        match self.0.as_mut().poll(act, ctx, task) {
            Poll::Pending => {
                if ctx.state().alive() {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            }
            Poll::Ready(_) => Poll::Ready(()),
        }
    }
}

pub(crate) struct ActorDelayedMessageItem<A, M>
where
    A: Actor,
    M: Message,
{
    msg: Option<M>,
    timeout: Delay,
    act: PhantomData<A>,
    m: PhantomData<M>,
}
impl<A, M> Unpin for ActorDelayedMessageItem<A, M>
where
    A: Actor,
    M: Message,
{
}

impl<A, M> ActorDelayedMessageItem<A, M>
where
    A: Actor,
    M: Message,
{
    pub fn new(msg: M, timeout: Duration) -> Self {
        Self {
            msg: Some(msg),
            timeout: tokio::time::delay_for(timeout),
            act: PhantomData,
            m: PhantomData,
        }
    }
}

impl<A, M> ActorFuture for ActorDelayedMessageItem<A, M>
where
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
    M: Message + 'static,
{
    type Output = ();
    type Actor = A;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.get_mut();
        ready!(Pin::new(&mut this.timeout).poll(task));
        let fut = A::handle(act, this.msg.take().unwrap(), ctx);
        fut.handle::<()>(ctx, None);
        Poll::Ready(())
    }
}

pub(crate) struct ActorMessageItem<A, M>
where
    A: Actor,
    M: Message,
{
    msg: Option<M>,
    act: PhantomData<A>,
}

impl<A: Actor, M: Message> Unpin for ActorMessageItem<A, M> {}

impl<A, M> ActorMessageItem<A, M>
where
    A: Actor,
    M: Message,
{
    pub fn new(msg: M) -> Self {
        Self {
            msg: Some(msg),
            act: PhantomData,
        }
    }
}

impl<A, M: 'static> ActorFuture for ActorMessageItem<A, M>
where
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
    M: Message,
{
    type Output = ();
    type Actor = A;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        _: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.get_mut();
        let fut = Handler::handle(act, this.msg.take().unwrap(), ctx);
        fut.handle::<()>(ctx, None);
        Poll::Ready(())
    }
}

#[pin_project]
pub(crate) struct ActorMessageStreamItem<A, M, S>
where
    A: Actor,
    M: Message,
{
    #[pin]
    stream: S,
    act: PhantomData<A>,
    msg: PhantomData<M>,
}

impl<A, M, S> ActorMessageStreamItem<A, M, S>
where
    A: Actor,
    M: Message,
{
    pub fn new(st: S) -> Self {
        Self {
            stream: st,
            act: PhantomData,
            msg: PhantomData,
        }
    }
}

impl<A, M: 'static, S> ActorFuture for ActorMessageStreamItem<A, M, S>
where
    S: Stream<Item = M>,
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
    M: Message,
{
    type Output = ();
    type Actor = A;

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        let mut this = self.project();
        loop {
            match this.stream.as_mut().poll_next(task) {
                Poll::Ready(Some(msg)) => {
                    let fut = Handler::handle(act, msg, ctx);
                    fut.handle::<()>(ctx, None);
                    if ctx.waiting() {
                        return Poll::Pending;
                    }
                }
                Poll::Ready(None) => return Poll::Ready(()),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
