//! Custom `Future` and `Stream` implementation with `Actix` support

pub mod future;
pub mod stream;
pub mod try_future;

pub use self::future::{
    result::{err, ok, ready, result, Ready},
    wrap_future, ActorFuture, ActorFutureExt, LocalBoxActorFuture, WrapFuture,
};
pub use self::stream::{wrap_stream, ActorStream, ActorStreamExt, WrapStream};
pub use self::try_future::{ActorTryFuture, ActorTryFutureExt};
