use std;
use std::rc::Rc;
use std::cell::RefCell;

use boxfnonce::BoxFnOnce;
use futures::{future, Future, Stream};
use tokio_core::reactor::Handle;

use context::Context;
use service::{Message, Service};

/// Service builder
pub struct Builder<T> where T: Service<Context=Context<T>> {
    ctx: Context<T>,
    factory: Option<BoxFnOnce<(Context<T>,)>>,
}

impl<T> Builder<T> where T: Service<Context=Context<T>>
{
    /// Build service for `T` and stream `S`
    // #[must_use = "service do nothing unless polled"]
    pub fn build<S>(srv: T, st: T::State, stream: S, handle: &Handle) -> Self
        where S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static,
    {
        Builder {
            ctx: Context::new(
                Rc::new(RefCell::new(st)), srv, stream, handle),
            factory: None
        }
    }

    /// Build service for `T` and stream `S`
    // #[must_use = "service do nothing unless polled"]
    pub fn with_service_init<S, F>(st: T::State, stream: S, handle: &Handle, f: F) -> Self
        where F: 'static + FnOnce(&mut Context<T>) -> T,
              S: Stream<Item=<T::Message as Message>::Item,
                        Error=<T::Message as Message>::Error> + 'static,
    {
        Builder {
            ctx: Context::new(Rc::new(RefCell::new(st)),
                              unsafe{std::mem::uninitialized()}, stream, handle),
            factory: Some(BoxFnOnce::from(|mut ctx| {
                let srv = f(&mut ctx);
                ctx.set_service(srv);
                ctx.run();
            }))
        }
    }

    /// Build service for `T` and stream `S`
    // #[must_use = "service do nothing unless polled"]
    pub fn from_context<C, S, F>(ctx: &Context<C>, stream: S, f: F) -> Self
        where C: Service<State=T::State, Context=Context<C>>,
              F: FnOnce(&mut Context<T>) -> T + 'static,
              S: Stream<Item=<<T as Service>::Message as Message>::Item,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        Builder {
            ctx: Context::new(ctx.clone(),
                              unsafe{std::mem::uninitialized()}, stream, ctx.handle()),
            factory: Some(BoxFnOnce::from(|mut ctx| {
                let srv = f(&mut ctx);
                ctx.set_service(srv);
                ctx.run();
            }))
        }
    }

    pub fn run(self) where Self: 'static, T: 'static {
        let handle: &Handle = unsafe{std::mem::transmute(&self.ctx.handle())};
        if let None = self.factory {
            self.ctx.run()
        } else {
            handle.spawn_fn(move || {
                let Builder { ctx, factory } = self;
                factory.unwrap().call(ctx);
                future::ok(())
            })
        }
    }

    pub fn clone_and_run(self) -> Rc<RefCell<T::State>> where Self: 'static, T: 'static
    {
        let st = self.ctx.clone();
        self.ctx.run();
        st
    }

    /// Add future
    // #[must_use = "service do nothing unless polled"]
    pub fn add_future<F>(mut self, fut: F) -> Self
        where F: Future<Item=<<T as Service>::Message as Message>::Item,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        self.ctx.add_future(fut);
        self
    }

    /// Add stream
    // #[must_use = "service do nothing unless polled"]
    pub fn add_stream<S>(mut self, fut: S) -> Self
        where S: Stream<Item=<<T as Service>::Message as Message>::Item,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        self.ctx.add_stream(fut);
        self
    }

    /// Add stream
    // #[must_use = "service do nothing unless polled"]
    pub fn add_fut_stream<F>(mut self, fut: F) -> Self
        where F: Future<Item=
                        Box<Stream<Item=<<T as Service>::Message as Message>::Item,
                                   Error=<<T as Service>::Message as Message>::Error>>,
                        Error=<<T as Service>::Message as Message>::Error> + 'static
    {
        self.ctx.add_fut_stream(fut);
        self
    }
}
