use std::future::Future;
use std::task::Poll;

use std::mem;

use crate::actor::Actor;
use crate::fut::ActorFuture;

#[derive(Debug)]
pub enum Chain<A, B, C>
where
    A: ActorFuture,
{
    First(A, C),
    Second(B),
    Done,
}
/*
impl<A, B, C> Chain<A, B, C>
where
    A: ActorFuture,
    B: ActorFuture<Actor = A::Actor>,
{
    pub fn new(a: A, c: C) -> Self {
        Chain::First(a, c)
    }

    pub fn poll<F>(
        &mut self,
        srv: &mut A::Actor,
        ctx: &mut <A::Actor as Actor>::Context,
        f: F,
    ) -> Poll<B::Item, B::Error>
    where
        F: FnOnce(
            Result<A::Item, A::Error>,
            C,
            &mut A::Actor,
            &mut <A::Actor as Actor>::Context,
        ) -> Result<Result<B::Item, B>, B::Error>,
    {
        let a_result = match *self {
            Chain::First(ref mut a, _) => match a.poll(srv, ctx) {
                Ok(Poll::Pending) => return Ok(Poll::Pending),
                Ok(Poll::Ready(t)) => Ok(t),
                Err(e) => Err(e),
            },
            Chain::Second(ref mut b) => return b.poll(srv, ctx),
            Chain::Done => panic!("cannot poll a chained future twice"),
        };
        let data = match mem::replace(self, Chain::Done) {
            Chain::First(_, c) => c,
            _ => panic!(),
        };
        match f(a_result, data, srv, ctx)? {
            Ok(e) => Ok(Poll::Ready(e)),
            Err(mut b) => {
                let ret = b.poll(srv, ctx);
                *self = Chain::Second(b);
                ret
            }
        }
    }
}
*/
