use fut::CtxFuture;

pub trait Item {
    type Item;
    type Error;
}

pub enum ServiceResult {
    NotReady,
    Done,
}

impl<T, E> Item for Result<T, E> {
    type Item=T;
    type Error=E;
}

pub type DefaultMessage = Result<(), ()>;

pub trait Service: Sized + 'static {

    type Context;
    type Message: Item;

    /// Method is called when service get polled first time.
    fn start(&mut self, &mut Self::Context) {}

    /// Method is called when context stream finishes.
    fn finished(&mut self, _ctx: &mut Self::Context) -> ServiceResult {
        ServiceResult::Done
    }

    /// Method is called for every item from the stream.
    fn call(&mut self,
            _ctx: &mut Self::Context,
            result: Result<<Self::Message as Item>::Item,
                           <Self::Message as Item>::Error>) -> ServiceResult {
        match result {
            Ok(_) => ServiceResult::NotReady,
            Err(_) => ServiceResult::Done
        }
    }
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
              srv: &mut <Self as Message>::Service,
              ctx: &mut <<Self as Message>::Service as Service>::Context) -> MessageFuture<Self>;
}
