use context::Context;
use message::MessageFuture;


#[doc(hidden)]
pub trait CtxActor {}


pub enum ActorStatus {
    NotReady,
    Done,
}

impl<T, E> Message for Result<T, E> where Self: Sized + 'static{
    type Item=T;
    type Error=E;
}

pub type DefaultMessage = Result<(), ()>;

#[allow(unused_variables)]
pub trait Actor: Sized + 'static {

    type Message: Message;

    /// Method is called when service get polled first time.
    fn start(&mut self, ctx: &mut Context<Self>) {}

    /// Method is called when context's stream finishes.
    /// By default returns `ActorStatus::Done`.
    fn finished(&mut self, ctx: &mut Context<Self>) -> ActorStatus {
        ActorStatus::Done
    }

    /// Method is called for every item from the stream.
    /// By default returns `ActorStatus::Done` for any error,
    /// and `ActorStatus::NotReady` for any other message
    fn call(&mut self,
            msg: Result<<Self::Message as Message>::Item, <Self::Message as Message>::Error>,
            ctx: &mut Context<Self>) -> ActorStatus {
        match msg {
            Ok(_) => ActorStatus::NotReady,
            Err(_) => ActorStatus::Done
        }
    }
}

/// Service is Actor
impl<T> CtxActor for T where T: Actor {}


pub trait Message: Sized + 'static {

    /// The type of value that this message will resolved with if it is successful.
    type Item;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error;

}

pub trait MessageHandler<M>
    where M: Message,
          Self: Actor,
{
    fn handle(&mut self, msg: M, ctx: &mut Context<Self>) -> MessageFuture<Self, M>;
}
