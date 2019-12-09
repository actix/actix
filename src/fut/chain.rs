use futures::ready;
use futures::task::{Context, Poll};
use std::mem;
use std::pin::Pin;

use pin_project::{pin_project, project};

use crate::actor::Actor;
use crate::fut::ActorFuture;

// TODO: Check pinning guarantees,
#[pin_project]
#[derive(Debug)]
pub enum Chain<A, B, C>
where
    A: ActorFuture,
{
    First(#[pin] A, Option<C>),
    Second(#[pin] B),
    Done,
}

impl<A, B, C> Chain<A, B, C>
where
    A: ActorFuture,
    B: ActorFuture<Actor = A::Actor>,
{
    pub fn new(a: A, c: C) -> Self {
        Chain::First(a, Some(c))
    }

    #[project]
    pub fn poll<F>(
        mut self: Pin<&mut Self>,
        srv: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        task: &mut Context<'_>,
        f: F,
    ) -> Poll<B::Item>
    where
        F: FnOnce(
            A::Item,
            C,
            &mut A::Actor,
            &mut <A::Actor as Actor>::Context,
        ) -> Result<B::Item, B>,
    {
        let mut this = self.as_mut();
        #[project]
        match this.project() {
            Chain::First(a, mut data) => {
                let a_res = ready!(a.poll(srv, ctx, task));
                return match f(a_res, data.take().unwrap(), srv, ctx) {
                    Ok(e) => Poll::Ready(e),
                    Err(mut b) => {
                        let ret =
                            unsafe { Pin::new_unchecked(&mut b) }.poll(srv, ctx, task);
                        self.set(Chain::Second(b));
                        ret
                    }
                };
            }
            Chain::Second(b) => return b.poll(srv, ctx, task),
            Done => panic!("cannot poll a chained future twice"),
        };
    }
}
