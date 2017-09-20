use futures::Poll;

pub trait Message {
    type Item;
    type Error;
}

impl<T, E> Message for Result<T, E> {
    type Item=T;
    type Error=E;
}

pub trait Service: Sized + 'static {

    type State;
    type Context;
    type Message: Message;
    type Result: Message;

    /// Method is called when service get polled first time.
    fn start(&mut self, &mut Self::State, &mut Self::Context) {}

    /// Method is called when context stream finishes.
    fn finished(&mut self, st: &mut Self::State, ctx: &mut Self::Context)
                -> Poll<<<Self as Service>::Result as Message>::Item,
                        <<Self as Service>::Result as Message>::Error>;

    /// Method is called for every item from the stream.
    fn call(&mut self,
            st: &mut Self::State,
            ctx: &mut Self::Context,
            result: Result<<Self::Message as Message>::Item, <Self::Message as Message>::Error>)
            -> Poll<<<Self as Service>::Result as Message>::Item,
                    <<Self as Service>::Result as Message>::Error>;
}
