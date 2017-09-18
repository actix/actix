use std::mem;
use futures::{Async, Poll};

use fut::CtxFuture;


#[derive(Debug)]
pub enum Chain<A, B, C> where A: CtxFuture {
    First(A, C),
    Second(B),
    Done,
}

impl<A, B, C> Chain<A, B, C>
    where A: CtxFuture,
          B: CtxFuture<Context=A::Context, Service=A::Service>,
{
    pub fn new(a: A, c: C) -> Chain<A, B, C> {
    Chain::First(a, c)
}

pub fn poll<F>(&mut self, ctx: &mut A::Context, srv: &mut A::Service, f: F)
               -> Poll<B::Item, B::Error>
        where F: FnOnce(Result<A::Item, A::Error>, C, &mut A::Context, &mut A::Service)
                        -> Result<Result<B::Item, B>, B::Error>,
    {
        let a_result = match *self {
            Chain::First(ref mut a, _) => {
                match a.poll(ctx, srv) {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Ok(Async::Ready(t)) => Ok(t),
                    Err(e) => Err(e),
                }
            }
            Chain::Second(ref mut b) => return b.poll(ctx, srv),
            Chain::Done => panic!("cannot poll a chained future twice"),
        };
        let data = match mem::replace(self, Chain::Done) {
            Chain::First(_, c) => c,
            _ => panic!(),
        };
        match try!(f(a_result, data, ctx, srv)) {
            Ok(e) => Ok(Async::Ready(e)),
            Err(mut b) => {
                let ret = b.poll(ctx, srv);
                *self = Chain::Second(b);
                ret
            }
        }
    }
}
