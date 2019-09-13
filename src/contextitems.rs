use std::marker::PhantomData;
use std::time::Duration;
use std::future::Future;
use std::task::Poll;

use futures::Stream;
use tokio_timer::Delay;

use crate::actor::{Actor, ActorContext, AsyncContext};
use crate::clock;
use crate::fut::ActorFuture;
use crate::handler::{Handler, Message, MessageResponse};
use std::task;
use std::pin::Pin;

pub(crate) struct ActorWaitItem<A: Actor>(
    Box<dyn ActorFuture<Item = (), Actor = A>>,
);


impl<A> ActorWaitItem<A>
where
    A: Actor,
    A::Context: ActorContext + AsyncContext<A>,
{
    #[inline]
    pub fn new<F>(fut: F) -> Self
    where
        F: ActorFuture<Item = (), Actor = A> + 'static,
    {
        ActorWaitItem(Box::new(fut))
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context, task : &mut task::Context<'_>) -> Poll<()> {
        match self.0.poll(act, ctx, task) {
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

impl<A, M> ActorDelayedMessageItem<A, M>
where
    A: Actor,
    M: Message,
{
    pub fn new(msg: M, timeout: Duration) -> Self {
        Self {
            msg: Some(msg),
            timeout: tokio_timer::delay(clock::now() + timeout),
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
    type Item = ();
    type Actor = A;

    fn poll(
        &mut self,
        act: &mut A,
        ctx: &mut A::Context,
        task : &mut task::Context<'_>
    ) -> Poll<Self::Item> {
        match unsafe { Pin::new_unchecked(&mut self.timeout) }.poll(task) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                let fut = A::handle(act, self.msg.take().unwrap(), ctx);
                fut.handle::<()>(ctx, None);
                Poll::Ready(())
            }
        }
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
    type Item = ();
    type Actor = A;

    fn poll(
        &mut self,
        act: &mut A,
        ctx: &mut A::Context,
        task : &mut task::Context<'_>
    ) -> Poll<Self::Item> {
        let fut = Handler::handle(act, self.msg.take().unwrap(), ctx);
        fut.handle::<()>(ctx, None);
        Poll::Ready(())
    }
}

pub(crate) struct ActorMessageStreamItem<A, M, S>
where
    A: Actor,
    M: Message,
{
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
    type Item = ();
    type Actor = A;

    fn poll(
        &mut self,
        act: &mut A,
        ctx: &mut A::Context,
        task : &mut task::Context<'_>
    ) -> Poll<Self::Item> {
        loop {
            match unsafe { Pin::new_unchecked(&mut self.stream) }.poll_next(task) {
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
