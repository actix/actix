use std;
use futures::{future, Stream};

use address::Address;
use context::Context;
use service::{Item, Service};
use system::get_handle;


pub trait ServiceBuilder<T> where T: Service + Sized + 'static {

    fn start(self) -> Address<T>;

    fn start_with<S>(self, stream: S) -> Address<T>
        where S: Stream<Item=<T::Message as Item>::Item,
                        Error=<T::Message as Item>::Error> + 'static;

    fn init<F>(f: F) -> Address<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T;

    fn init_with<S, F>(stream: S, f: F) -> Address<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T,
              S: Stream<Item=<T::Message as Item>::Item,
                        Error=<T::Message as Item>::Error> + 'static;

    fn from_context<C, S, F>(ctx: &Context<C>, stream: S, f: F) -> Address<T>
        where C: Service,
              F: FnOnce(&mut Context<T>) -> T + 'static,
              S: Stream<Item=<<T as Service>::Message as Item>::Item,
                        Error=<<T as Service>::Message as Item>::Error> + 'static;
}

impl<T> ServiceBuilder<T> for T where T: Service {
    fn start(self) -> Address<T> {
        Context::new_empty(self, get_handle()).run()
    }

    fn start_with<S>(self, stream: S) -> Address<T>
        where S: Stream<Item=<T::Message as Item>::Item,
                        Error=<T::Message as Item>::Error> + 'static,
    {
        Context::new(self, stream, get_handle()).run()
    }

    fn init<F>(f: F) -> Address<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T
    {
        let mut ctx = Context::new_empty(unsafe{std::mem::uninitialized()}, get_handle());
        let addr = ctx.address();

        let handle = ctx.handle().clone();
        handle.spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_service(srv);
            std::mem::forget(old);
            ctx.run();
            future::ok(())
        });
        addr
    }

    fn init_with<S, F>(stream: S, f: F) -> Address<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T,
              S: Stream<Item=<T::Message as Item>::Item,
                        Error=<T::Message as Item>::Error> + 'static
    {
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()}, stream, get_handle());
        let addr = ctx.address();

        let handle = ctx.handle().clone();
        handle.spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_service(srv);
            std::mem::forget(old);
            ctx.run();
            future::ok(())
        });
        addr
    }

    /// Build service for `T` and stream `S`
    fn from_context<C, S, F>(ctx: &Context<C>, stream: S, f: F) -> Address<T>
        where C: Service,
              F: FnOnce(&mut Context<T>) -> T + 'static,
              S: Stream<Item=<<T as Service>::Message as Item>::Item,
                        Error=<<T as Service>::Message as Item>::Error> + 'static
    {
        let mut new = Context::new(unsafe{std::mem::uninitialized()}, stream, ctx.handle());
        let addr = new.address();

        let handle = ctx.handle().clone();
        handle.spawn_fn(move || {
            let srv = f(&mut new);
            let old = new.replace_service(srv);
            std::mem::forget(old);
            new.run();
            future::ok(())
        });
        addr
    }
}
