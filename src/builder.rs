use std;
use futures::{future, Stream};

use actor::{Actor, MessageHandler, StreamHandler};
use address::{Address, SyncAddress};
use arbiter::Arbiter;
use context::Context;


pub trait ServiceBuilder<A> where A: Actor + Sized + 'static {

    fn start(self) -> Address<A>;

    fn sync_start(self) -> SyncAddress<A>;

    fn start_with<S, E: 'static>(self, stream: S) -> Address<A>
        where S: Stream<Error=E> + 'static,
              S::Item: 'static,
              A: MessageHandler<S::Item, InputError=E> + StreamHandler<S::Item, InputError=E>;

    fn init<F>(f: F) -> Address<A>
        where F: 'static + FnOnce(&mut Context<A>) -> A;

    fn sync_init<F>(f: F) -> SyncAddress<A>
        where F: 'static + FnOnce(&mut Context<A>) -> A;

    fn init_with<S, F, E: 'static>(stream: S, f: F) -> Address<A>
        where F: 'static + FnOnce(&mut Context<A>) -> A,
              S: Stream<Error=E> + 'static,
              S::Item: 'static,
              A: MessageHandler<S::Item, InputError=E> + StreamHandler<S::Item, InputError=E>;
}

impl<A> ServiceBuilder<A> for A where A: Actor
{
    fn start(self) -> Address<A> {
        Context::new(self).run(Arbiter::handle())
    }

    fn sync_start(self) -> SyncAddress<A> {
        let mut ctx = Context::new(self);
        let addr = ctx.sync_address();
        ctx.run(Arbiter::handle());
        addr
    }

    fn start_with<S, E: 'static>(self, stream: S) -> Address<A>
        where S: Stream<Error=E> + 'static,
              S::Item: 'static,
              A: MessageHandler<S::Item, InputError=E> + StreamHandler<S::Item, InputError=E>,
    {
        let mut ctx = Context::new(self);
        ctx.add_stream(stream);
        ctx.run(Arbiter::handle())
    }

    fn init<F>(f: F) -> Address<A>
        where F: 'static + FnOnce(&mut Context<A>) -> A
    {
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()});
        let addr = ctx.address();

        Arbiter::handle().spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_actor(srv);
            std::mem::forget(old);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }

    fn sync_init<F>(f: F) -> SyncAddress<A>
        where F: 'static + FnOnce(&mut Context<A>) -> A
    {
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()});
        let addr = ctx.sync_address();

        Arbiter::handle().spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_actor(srv);
            std::mem::forget(old);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }

    fn init_with<S, F, E: 'static>(stream: S, f: F) -> Address<A>
        where F: 'static + FnOnce(&mut Context<A>) -> A,
              S: Stream<Error=E> + 'static,
              S::Item: 'static,
              A: MessageHandler<S::Item, InputError=E> + StreamHandler<S::Item, InputError=E>,
    {
        let mut ctx = Context::new(unsafe{std::mem::uninitialized()});
        let addr = ctx.address();

        Arbiter::handle().spawn_fn(move || {
            let srv = f(&mut ctx);
            let old = ctx.replace_actor(srv);
            std::mem::forget(old);
            ctx.add_stream(stream);
            ctx.run(Arbiter::handle());
            future::ok(())
        });
        addr
    }
}
