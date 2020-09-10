use std::{pin::Pin, task, task::Poll};

use crate::{
    actor::Actor,
    fut::{self, IntoActorFuture},
    ActorFuture,
};

mod map_err;
mod map_ok;

pub use map_err::*;
pub use map_ok::*;

pub trait ActorTryFuture: ActorFuture {
    type Ok;
    type Error;

    fn try_poll(
        self: Pin<&mut Self>,
        srv: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<Result<Self::Ok, Self::Error>>;
}

// TODO: seal ActorTryFuture trait
// mod private_try_future {
//     use super::{Actor, ActorFuture};
//     pub trait Sealed {}
//     impl<F, T, E, A> Sealed for F
//     where
//         A: Actor,
//         F: ?Sized + ActorFuture<Output = Result<T, E>, Actor = A>,
//     {
//     }
// }

impl<F, T, E, A> ActorTryFuture for F
where
    F: ActorFuture<Output = Result<T, E>, Actor = A> + ?Sized,
    A: Actor,
{
    type Ok = T;
    type Error = E;

    fn try_poll(
        self: Pin<&mut Self>,
        srv: &mut Self::Actor,
        ctx: &mut <Self::Actor as Actor>::Context,
        task: &mut task::Context<'_>,
    ) -> Poll<F::Output> {
        self.poll(srv, ctx, task)
    }
}

pub trait ActorTryFutureExt: ActorTryFuture {
    fn map_ok<R, F>(self, mapper: F) -> MapOk<Self, F>
    where
        F: FnOnce(Self::Ok, &mut Self::Actor, &mut <Self::Actor as Actor>::Context) -> R,
        Self: Sized,
    {
        MapOk::new(self, mapper)
    }

    fn map_err<R, F>(self, mapper: F) -> MapErr<Self, F>
    where
        F: FnOnce(
            Self::Error,
            &mut Self::Actor,
            &mut <Self::Actor as Actor>::Context,
        ) -> R,
        Self: Sized,
    {
        MapErr::new(self, mapper)
    }
}

impl<F: ActorTryFuture + ?Sized> ActorTryFutureExt for F {}
