use context::Context;
use message::MessageFuture;


#[doc(hidden)]
pub trait ActixActor {}

#[allow(unused_variables)]
pub trait Actor: Sized + 'static {

    /// Method is called when actor get polled first time.
    fn started(&mut self, ctx: &mut Context<Self>) {}

    /// Method is called when context's stream finishes.
    /// By default returns `ActorStatus::Done`.
    fn finished(&mut self, ctx: &mut Context<Self>) {}
}

/// Service is Actor
impl<T> ActixActor for T where T: Actor {}


pub trait Message: Sized + 'static {

    /// The type of value that this message will resolved with if it is successful.
    type Item;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error;

}

#[allow(unused_variables)]
pub trait MessageHandler<M> where Self: Actor
{
    /// The type of value that this message will resolved with if it is successful.
    type Item;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error;

    /// If message handler is used for handling messages from Future or Stream, then
    /// `InputError` type has to be set to correspondent `Error`
    type InputError;

    /// Method is called on error.
    fn error(&mut self, err: Self::InputError, ctx: &mut Context<Self>) {}

    /// Method is called for every message received by this Actor
    fn handle(&mut self, msg: M, ctx: &mut Context<Self>) -> MessageFuture<Self, M>;
}

#[allow(unused_variables)]
pub trait StreamHandler<M>: MessageHandler<M>
    where Self: Actor
{
    /// Method is called when service get polled first time.
    fn started(&mut self, ctx: &mut Context<Self>) {}

    /// Method is called when stream finishes.
    fn finished(&mut self, ctx: &mut Context<Self>) {}

}
