use std::{
    future::Future,
    pin::Pin,
    task::{self, Poll},
    time::Duration,
};

use futures_core::{ready, stream::Stream};
use pin_project_lite::pin_project;

use crate::{
    actor::{Actor, ActorContext, AsyncContext},
    clock::Sleep,
    fut::ActorFuture,
    handler::{Handler, Message, MessageResponse},
};

pub(crate) struct ActorWaitItem<A: Actor>(Pin<Box<dyn ActorFuture<A, Output = ()>>>);

impl<A> ActorWaitItem<A>
where
    A: Actor,
    A::Context: ActorContext + AsyncContext<A>,
{
    #[inline]
    pub fn new<F>(fut: F) -> Self
    where
        F: ActorFuture<A, Output = ()> + 'static,
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

pin_project! {
    pub(crate) struct ActorDelayedMessageItem<M: Message>{
        msg: Option<M>,
        #[pin]
        timeout: Sleep,
    }
}

impl<M: Message> ActorDelayedMessageItem<M> {
    pub fn new(msg: M, timeout: Duration) -> Self {
        Self {
            msg: Some(msg),
            timeout: actix_rt::time::sleep(timeout),
        }
    }
}

impl<A, M> ActorFuture<A> for ActorDelayedMessageItem<M>
where
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
    M: Message + 'static,
{
    type Output = ();

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.project();
        ready!(this.timeout.poll(task));
        let fut = A::handle(act, this.msg.take().unwrap(), ctx);
        fut.handle(ctx, None);
        Poll::Ready(())
    }
}

pub(crate) struct ActorMessageItem<M: Message> {
    msg: Option<M>,
}

impl<M: Message> Unpin for ActorMessageItem<M> {}

impl<M: Message> ActorMessageItem<M> {
    pub fn new(msg: M) -> Self {
        Self { msg: Some(msg) }
    }
}

impl<A, M> ActorFuture<A> for ActorMessageItem<M>
where
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
    M: Message + 'static,
{
    type Output = ();

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        _: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        let this = self.get_mut();
        let fut = Handler::handle(act, this.msg.take().unwrap(), ctx);
        fut.handle(ctx, None);
        Poll::Ready(())
    }
}

pin_project! {
    pub(crate) struct ActorMessageStreamItem<S>{
        #[pin]
        stream: S,
    }
}

impl<S> ActorMessageStreamItem<S> {
    pub fn new(st: S) -> Self {
        Self { stream: st }
    }
}

impl<A, S> ActorFuture<A> for ActorMessageStreamItem<S>
where
    S: Stream,
    A: Actor + Handler<S::Item>,
    A::Context: AsyncContext<A>,
    S::Item: Message + 'static,
{
    type Output = ();

    fn poll(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        let mut this = self.project();

        while let Some(msg) = ready!(this.stream.as_mut().poll_next(task)) {
            let fut = Handler::handle(act, msg, ctx);
            fut.handle(ctx, None);
            if ctx.waiting() {
                return Poll::Pending;
            }
        }

        Poll::Ready(())
    }
}
