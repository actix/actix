use std;

use boxfnonce::BoxFnOnce;
use futures::{future, Future, Stream};
use tokio_core::reactor::Handle;

use address::Address;
use context::Context;
use service::{Item, Service};

/// Service builder
pub struct Builder<T> where T: Service<Context=Context<T>> {
    ctx: Context<T>,
    factory: Option<BoxFnOnce<(Context<T>,)>>,
}

impl<T> Builder<T> where T: Service<Context=Context<T>>
{
    /// Build service for `T` and stream `S`
    // #[must_use = "service do nothing unless polled"]
    pub fn build<S>(srv: T, stream: S, handle: &Handle) -> Self
        where S: Stream<Item=<T::Message as Item>::Item,
                        Error=<T::Message as Item>::Error> + 'static,
    {
        Builder {
            ctx: Context::new(srv, stream, handle),
            factory: None
        }
    }

    /// Build service for `T` and stream `S`
    // #[must_use = "service do nothing unless polled"]
    pub fn with_service_init<S, F>(stream: S, handle: &Handle, f: F) -> Self
        where F: 'static + FnOnce(&mut Context<T>) -> T,
              S: Stream<Item=<T::Message as Item>::Item,
                        Error=<T::Message as Item>::Error> + 'static,
    {
        Builder {
            ctx: Context::new(unsafe{std::mem::uninitialized()}, stream, handle),
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
        where C: Service<Context=Context<C>>,
              F: FnOnce(&mut Context<T>) -> T + 'static,
              S: Stream<Item=<<T as Service>::Message as Item>::Item,
                        Error=<<T as Service>::Message as Item>::Error> + 'static
    {
        Builder {
            ctx: Context::new(unsafe{std::mem::uninitialized()}, stream, ctx.handle()),
            factory: Some(BoxFnOnce::from(|mut ctx| {
                let srv = f(&mut ctx);
                ctx.set_service(srv);
                ctx.run();
            }))
        }
    }

    pub fn run(self) -> Address<T> where Self: 'static, T: 'static
    {
        let addr = self.ctx.address();

        if let None = self.factory {
            self.ctx.run()
        } else {
            let Builder { ctx, factory } = self;
            let handle = ctx.handle().clone();
            handle.spawn_fn(move || {
                factory.unwrap().call(ctx);
                future::ok(())
            })
        }

        addr
    }

    /// Add future
    // #[must_use = "service do nothing unless polled"]
    pub fn add_future<F>(mut self, fut: F) -> Self
        where F: Future<Item=<<T as Service>::Message as Item>::Item,
                        Error=<<T as Service>::Message as Item>::Error> + 'static
    {
        self.ctx.add_future(fut);
        self
    }

    /// Add stream
    // #[must_use = "service do nothing unless polled"]
    pub fn add_stream<S>(mut self, fut: S) -> Self
        where S: Stream<Item=<<T as Service>::Message as Item>::Item,
                        Error=<<T as Service>::Message as Item>::Error> + 'static
    {
        self.ctx.add_stream(fut);
        self
    }

    /// Add stream
    // #[must_use = "service do nothing unless polled"]
    pub fn add_fut_stream<F>(mut self, fut: F) -> Self
        where F: Future<Item=
                        Box<Stream<Item=<<T as Service>::Message as Item>::Item,
                                   Error=<<T as Service>::Message as Item>::Error>>,
                        Error=<<T as Service>::Message as Item>::Error> + 'static
    {
        self.ctx.add_fut_stream(fut);
        self
    }
}
