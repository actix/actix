//! Mocking utility actor.
//!
//! This actor wraps any actor, and replaces instances of that actor with
//! mocker actor, which is able to accept all messages which the actor can
//! receive.
//!
//! Mocking is intended to be achieved by using a pattern similar to
//! ```ignore
//! #[cfg(not(test))]
//! type DBClientAct = DBClientActor;
//! #[cfg(test)]
//! type DBClientAct = Mocker<DBClientActor>;
//! ```
//! Then, the actor should be used as a system service (or arbiter service, but
//! take care that all the places which will use the mocked actor are on the
//! same arbiter). Thus, in a test, it will retrieve the mocker from the
//! registry instead of the actual actor.
//!
//! To set the mock function in the actor, the `init_actor` function
//! is used, which allows the state of an actor to be set when it is
//! started as an arbiter or system service. A closure which takes
//! `Box<Any>` is evaluated for every message, and must return
//! `Box<Any>`, specifically the return type for the message type
//! send.
//!
//! See the mock example to see how it can be used.

use std::any::{self, Any};
use std::marker::PhantomData;

use crate::handler::MessageResponse;
use crate::prelude::*;

/// This actor is able to wrap another actor and accept all the messages the
/// wrapped actor can, passing it to a closure which can mock the response of
/// the actor.
#[allow(clippy::type_complexity)]
pub struct Mocker<T: Sized + Unpin + 'static> {
    phantom: PhantomData<T>,
    mock: Box<dyn FnMut(Box<dyn Any>, &mut Context<Mocker<T>>) -> Box<dyn Any>>,
}

impl<T: Unpin> Mocker<T> {
    #[allow(clippy::type_complexity)]
    pub fn mock(
        mock: Box<dyn FnMut(Box<dyn Any>, &mut Context<Mocker<T>>) -> Box<dyn Any>>,
    ) -> Mocker<T> {
        Mocker::<T> {
            phantom: PhantomData,
            mock,
        }
    }

    /// A helper for calling [`Mocker::mock()`] when it is known that only
    /// one message type will ever be sent to the [`Mocker`].
    pub fn mock_one<F, M>(mut mock: F) -> Mocker<T>
    where
        M: Message + 'static,
        F: FnMut(M, &mut Context<Mocker<T>>) -> M::Result + 'static,
    {
        Mocker::mock(Box::new(move |msg, ctx| match msg.downcast::<M>() {
            Ok(msg) => Box::new(mock(*msg, ctx)),
            Err(_) => panic!(
                "This mocker can only handle messages of type {}",
                any::type_name::<M>()
            ),
        }))
    }
}

impl<T: SystemService> SystemService for Mocker<T> {}
impl<T: ArbiterService> ArbiterService for Mocker<T> {}
impl<T: Unpin> Supervised for Mocker<T> {}

impl<T: Unpin> Default for Mocker<T> {
    fn default() -> Self {
        panic!("Mocker actor used before set")
    }
}

impl<T: Sized + Unpin + 'static> Actor for Mocker<T> {
    type Context = Context<Self>;
}

impl<M: 'static, T: Sized + Unpin + 'static> Handler<M> for Mocker<T>
where
    M: Message,
    <M as Message>::Result: MessageResponse<Mocker<T>, M>,
{
    type Result = M::Result;
    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> M::Result {
        let ret = (self.mock)(Box::new(msg), ctx);

        match ret.downcast::<M::Result>() {
            // The result was returned directly.
            Ok(response) => *response,
            // Note: We also handle the case where Option<M::Result> is returned
            // for backwards compatibility.
            Err(other) => match other.downcast::<Option<M::Result>>() {
                Ok(mut response) => response.take().expect("No response provided"),
                // We got something else, blow up with a useful error message.
                Err(_) => {
                    panic!(
                        "Wrong return type for message. Expected a {} when responding to {}",
                        any::type_name::<M::Result>(),
                        any::type_name::<M>(),
                    );
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dev::ResponseChannel;

    struct ActorBeingMocked;

    impl Actor for ActorBeingMocked {
        type Context = Context<Self>;
    }

    #[derive(Debug, PartialEq)]
    struct Ping(u32);

    impl Message for Ping {
        type Result = Pong;
    }

    #[derive(Debug, PartialEq)]
    struct Pong(u32);

    impl<A: Actor> MessageResponse<A, Ping> for Pong {
        fn handle<R: ResponseChannel<Ping>>(self, _ctx: &mut A::Context, tx: Option<R>) {
            if let Some(tx) = tx {
                tx.send(self);
            }
        }
    }

    impl Handler<Ping> for ActorBeingMocked {
        type Result = MessageResult<Ping>;

        fn handle(&mut self, _msg: Ping, _ctx: &mut Self::Context) -> Self::Result {
            todo!()
        }
    }

    #[test]
    #[should_panic(
        expected = "Wrong return type for message. Expected a actix::actors::mocker::tests::Pong when responding to actix::actors::mocker::tests::Ping"
    )]
    fn users_get_a_useful_panic_message_when_mocking_fails() {
        System::new("test").block_on(async {
            let bad_mocker =
                Mocker::<ActorBeingMocked>::mock(Box::new(|_, _| Box::new(42))).start();

            let _response = bad_mocker.send(Ping(1)).await.unwrap();
        });
    }

    #[test]
    fn ping_pong_with_generic_mock_function() {
        fn example_handler<A: Actor>(
            msg: Box<dyn Any>,
            _ctx: &mut Context<Mocker<A>>,
        ) -> Box<dyn Any> {
            let msg = msg.downcast::<Ping>().expect("Unable to mock this message");

            Box::new(Pong(msg.0 + 123))
        }

        System::new("test").block_on(async {
            let mocker =
                Mocker::<ActorBeingMocked>::mock(Box::new(example_handler)).start();

            let response = mocker.send(Ping(1)).await.unwrap();

            assert_eq!(response, Pong(1 + 123));
        });
    }

    #[test]
    fn ping_pong_with_mock_one() {
        System::new("test").block_on(async {
            let mocker =
                Mocker::<ActorBeingMocked>::mock_one(|msg: Ping, _| Pong(msg.0 + 123))
                    .start();

            let response = mocker.send(Ping(1)).await.unwrap();

            assert_eq!(response, Pong(1 + 123));
        });
    }
}
