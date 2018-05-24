//! Mocking utility actor
//! This actor wraps any actor, and replaces instances of that actor with
//! mocker actor, which is able to accept all messages which the actor can
//! receive.
//!
//! Mocking is intended to be achieved by using a pattern similar to
//! ```rust,ignore
//! #[cfg(not(test))]
//! type DBClientAct = DBClientActor;
//! #[cfg(test)]
//! type DBClientAct = Mocker<DBClientActor>;
//! ```
//! Then, the actor should be used as a System Service (or Arbiter Service, but
//! take care that all the places which will use the mocked actor is on the
//! same arbiter). Thus, in a test, it will retrieve the mocker from the
//! registry instead of the actual actor.
//!
//! To set the mock function in the actor, the init_actor function is used,
//! which allows the state of an actor to be set when it is started as a
//! arbiter or system service. A closure which takes Box<Any> is evaluated for
//! every message, and must return Box<Any>, specifically the return type
//! for the message type send.
//!
//! See the mock example to see how it can be used.

use std::any::Any;
use std::marker::PhantomData;
use std::mem;

use handler::MessageResponse;
use prelude::*;

/// This actor is able to wrap another actor and accept all the messages the
/// wrapped actor can, passing it to a closure which can mock the response of
/// the actors
pub struct Mocker<T: Sized + 'static> {
    phantom: PhantomData<T>,
    mock: Box<FnMut(Box<Any>, &mut Context<Mocker<T>>) -> Box<Any>>,
}

impl<T> Mocker<T> {
    pub fn mock(
        mock: Box<FnMut(Box<Any>, &mut Context<Mocker<T>>) -> Box<Any>>,
    ) -> Mocker<T> {
        Mocker::<T> {
            phantom: PhantomData,
            mock,
        }
    }
}

impl<T: SystemService> actix::SystemService for Mocker<T> {}
impl<T: ArbiterService> actix::ArbiterService for Mocker<T> {}
impl<T> Supervised for Mocker<T> {}

impl<T> Default for Mocker<T> {
    fn default() -> Self {
        panic!("Mocker actor used before set")
    }
}

impl<T: Sized + 'static> Actor for Mocker<T> {
    type Context = Context<Self>;
}

impl<M: 'static, T: Sized + 'static> Handler<M> for Mocker<T>
where
    M: Message,
    <M as actix::Message>::Result: MessageResponse<Mocker<T>, M>,
{
    type Result = M::Result;
    fn handle(&mut self, msg: M, ctx: &mut Self::Context) -> M::Result {
        let mut ret = (self.mock)(Box::new(msg), ctx);
        let out = mem::replace(
            ret.downcast_mut::<Option<M::Result>>()
                .expect("wrong return type for message"),
            None,
        );
        match out {
            Some(a) => a,
            _ => panic!(),
        }
    }
}
