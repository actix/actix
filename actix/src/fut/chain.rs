use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::actor::Actor;
use crate::fut::ActorFuture;

pin_project! {
    #[project = ChainProj]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    #[derive(Debug)]
    pub enum Chain<A, B, Fn, Act> {
        First {
            #[pin]
            fut1: A,
            data: Option<Fn>,
            _act: PhantomData<Act>,
        },
        Second {
            #[pin]
            fut2: B
        },
        Empty,
    }
}

impl<A, B, Fn, Act> Chain<A, B, Fn, Act>
where
    A: ActorFuture<Act>,
    B: ActorFuture<Act>,
    Act: Actor,
{
    pub fn new(fut1: A, data: Fn) -> Self {
        Chain::First {
            fut1,
            data: Some(data),
            _act: PhantomData,
        }
    }

    pub fn poll<Fn1>(
        mut self: Pin<&mut Self>,
        srv: &mut Act,
        ctx: &mut Act::Context,
        task: &mut Context<'_>,
        f: Fn1,
    ) -> Poll<B::Output>
    where
        Fn1: FnOnce(A::Output, Fn, &mut Act, &mut Act::Context) -> B,
    {
        let mut f = Some(f);

        loop {
            let this = self.as_mut().project();
            let (output, data) = match this {
                ChainProj::First { fut1, data, .. } => {
                    let output = match fut1.poll(srv, ctx, task) {
                        Poll::Ready(t) => t,
                        Poll::Pending => return Poll::Pending,
                    };
                    (output, data.take().unwrap())
                }
                ChainProj::Second { fut2 } => {
                    return fut2.poll(srv, ctx, task);
                }
                ChainProj::Empty => unreachable!(),
            };

            self.set(Chain::Empty);
            let fut2 = (f.take().unwrap())(output, data, srv, ctx);
            self.set(Chain::Second { fut2 })
        }
    }
}
