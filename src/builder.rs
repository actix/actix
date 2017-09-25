use std;
use futures::{future, Stream};

use address::{Address, SyncAddress};
use arbiter::Arbiter;
use context::Context;
use service::{Message, Service};


pub trait ServiceBuilder<T> where T: Service + Sized + 'static {

    fn start(self) -> Address<T>;

    fn sync_start(self) -> SyncAddress<T>;

    fn start_with<S>(self, stream: S) -> Address<T>
        where S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static;

    fn init<F>(f: F) -> Address<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T;

    fn sync_init<F>(f: F) -> SyncAddress<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T;

    fn init_with<S, F>(stream: S, f: F) -> Address<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T,
              S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static;
}

impl<T> ServiceBuilder<T> for T where T: Service
{
    fn start(self) -> Address<T> {
        Context::new_empty(self).run(Arbiter::handle())
    }

    fn sync_start(self) -> SyncAddress<T> {
        let mut ctx = Context::new_empty(self);
        let addr = ctx.sync_address();
        ctx.run(Arbiter::handle());
        addr
    }

    fn start_with<S>(self, stream: S) -> Address<T>
        where S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static,
    {
        Context::new(self, stream).run(Arbiter::handle())
    }

    fn init<F>(f: F) -> Address<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T
    {
        let mut ctx = Context::new_empty(
            unsafe{std::mem::uninitialized()});
        let addr = ctx.address();

        Arbiter::handle().spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_service(srv);
            std::mem::forget(old);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }

    fn sync_init<F>(f: F) -> SyncAddress<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T
    {
        let mut ctx = Context::new_empty(unsafe{std::mem::uninitialized()});
        let addr = ctx.sync_address();

        Arbiter::handle().spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_service(srv);
            std::mem::forget(old);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }

    fn init_with<S, F>(stream: S, f: F) -> Address<T>
        where F: 'static + FnOnce(&mut Context<T>) -> T,
              S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static
    {
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()}, stream);
        let addr = ctx.address();

        Arbiter::handle().spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_service(srv);
            std::mem::forget(old);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }
}
