//! Custom `Future` and `Stream` implementation with `actix` support.

pub mod future;
pub mod stream;
pub mod try_future;

pub use self::{
    future::{
        result::{err, ok, ready, result, Ready},
        wrap_future, ActorFuture, ActorFutureExt, LocalBoxActorFuture, WrapFuture,
    },
    stream::{wrap_stream, ActorStream, ActorStreamExt, WrapStream},
    try_future::{ActorTryFuture, ActorTryFutureExt},
};
