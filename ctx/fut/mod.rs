use std::marker::PhantomData;
use futures::{future, Poll};

mod chain;
mod and_then;
mod result;
mod then;
mod map;
mod map_err;

pub use self::and_then::AndThen;
pub use self::then::Then;
pub use self::map::Map;
pub use self::map_err::MapErr;
pub use self::result::{result, ok, err, FutureResult};


pub trait CtxFuture {

    /// The type of value that this future will resolved with if it is
    /// successful.
    type Item;

    /// The type of error that this future will resolve with if it fails in a
    /// normal fashion.
    type Error;

    /// The context of this future
    type Context;

    /// The service within which this future runs
    type Service;

    fn poll(&mut self, ctx: &mut Self::Context, srv: &mut Self::Service)
            -> Poll<Self::Item, Self::Error>;

    fn map<F, U>(self, f: F) -> Map<Self, F>
        where F: FnOnce(Self::Item, &mut Self::Context, &mut Self::Service) -> U,
              Self: Sized,
    {
        map::new(self, f)
    }

    fn map_err<F, E>(self, f: F) -> MapErr<Self, F>
        where F: FnOnce(Self::Error, &mut Self::Context, &mut Self::Service) -> E,
              Self: Sized,
    {
        map_err::new(self, f)
    }

    fn then<F, B>(self, f: F) -> Then<Self, B, F>
        where F: FnOnce(Result<Self::Item, Self::Error>,
                        &mut Self::Context, &mut Self::Service) -> B,
              B: IntoCtxFuture<Context=Self::Context, Service=Self::Service>,
              Self: Sized,
    {
        then::new(self, f)
    }


    /// Execute another future after this one has resolved successfully.
    fn and_then<F, B>(self, f: F) -> AndThen<Self, B, F>
        where F: FnOnce(Self::Item, &mut Self::Context, &mut Self::Service) -> B,
              B: IntoCtxFuture<Error=Self::Error, Context=Self::Context, Service=Self::Service>,
              Self: Sized,
    {
        and_then::new(self, f)
    }
}


/// Class of types which can be converted into a future.
///
/// This trait is very similar to the `IntoIterator` trait and is intended to be
/// used in a very similar fashion.
pub trait IntoCtxFuture {
    /// The future that this type can be converted into.
    type Future: CtxFuture<Item=Self::Item, Error=Self::Error,
                           Context=Self::Context, Service=Self::Service>;

    /// The item that the future may resolve with.
    type Item;
    /// The error that the future may resolve with.
    type Error;
    /// The context of this future
    type Context;
    /// The service within which this future runs
    type Service;

    /// Consumes this object and produces a future.
    fn into_future(self) -> Self::Future;
}

impl<F: CtxFuture> IntoCtxFuture for F {
    type Future = F;
    type Item = F::Item;
    type Error = F::Error;
    type Context = F::Context;
    type Service = F::Service;

    fn into_future(self) -> F {
        self
    }
}

pub trait WrapFuture<C, S> {
    /// The future that this type can be converted into.
    type Future: CtxFuture<Item=Self::Item, Error=Self::Error,
                           Context=C, Service=S>;

    /// The item that the future may resolve with.
    type Item;
    /// The error that the future may resolve with.
    type Error;

    fn wrap(self) -> Self::Future;
}

impl<F: future::Future, C, S> WrapFuture<C, S> for F {
    type Future = FutureWrap<F, C, S>;
    type Item = F::Item;
    type Error = F::Error;

    fn wrap(self) -> Self::Future {
        wrap_future(self)
    }
}

pub struct FutureWrap<F, C, S> where F: future::Future {
    fut: F,
    ctx: PhantomData<C>,
    srv: PhantomData<S>,
}

pub fn wrap_future<F, C, S>(f: F) -> FutureWrap<F, C, S>
    where F: future::Future
{
    FutureWrap{fut: f, ctx: PhantomData, srv: PhantomData}
}

impl<F, C, S> CtxFuture for FutureWrap<F, C, S>
    where F: future::Future,
{
    type Item = F::Item;
    type Error = F::Error;
    type Context = C;
    type Service = S;

    fn poll(&mut self, _: &mut Self::Context, _: &mut Self::Service)
            -> Poll<Self::Item, Self::Error>
    {
        self.fut.poll()
    }
}
