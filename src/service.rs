use futures::Poll;
use fut::CtxFuture;

pub trait Item {
    type Item;
    type Error;
}

impl<T, E> Item for Result<T, E> {
    type Item=T;
    type Error=E;
}

pub trait Service: Sized + 'static {

    type State;
    type Context;
    type Message: Item;
    type Result: Item;

    /// Method is called when service get polled first time.
    fn start(&mut self, &mut Self::State, &mut Self::Context) {}

    /// Method is called when context stream finishes.
    fn finished(&mut self, st: &mut Self::State, ctx: &mut Self::Context)
                -> Poll<<<Self as Service>::Result as Item>::Item,
                        <<Self as Service>::Result as Item>::Error>;

    /// Method is called for every item from the stream.
    fn call(&mut self,
            st: &mut Self::State,
            ctx: &mut Self::Context,
            result: Result<<Self::Message as Item>::Item, <Self::Message as Item>::Error>)
            -> Poll<<<Self as Service>::Result as Item>::Item,
                    <<Self as Service>::Result as Item>::Error>;
}


pub type MessageFuture<T: Message> =
    Box<CtxFuture<Item=T::Item, Error=T::Error, Service=T::Service>>;


pub trait Message: Sized + 'static {

    /// The type of value that this message will resolved with if it is successful.
    type Item;

    /// The type of error that this message will resolve with if it fails in a normal fashion.
    type Error;

    type Service: Service;

    fn handle(&self,
              st: &mut <<Self as Message>::Service as Service>::State,
              srv: &mut <Self as Message>::Service,
              ctx: &mut <<Self as Message>::Service as Service>::Context) -> MessageFuture<Self>;
}
