# Getting Started

Let’s create and run our first actix application. We’ll create a new Cargo project
that depends on actix and then run the application.

In previous section we already installed required rust version. Now let's create new cargo projects.

## Ping actor

Let’s write our first actix application! Start by creating a new binary-based
Cargo project and changing into the new directory:

```bash
cargo new actor-ping --bin
cd actor-ping
```

Now, add actix as dependencies of your project by ensuring your Cargo.toml 
contains the following:

```toml
[dependencies]
actix = "0.5"
```

Let's create an actor that will accept `Ping` message and respond with number of pings processed.

An actor is a type that implements `Actor` trait:

```rust
# extern crate actix;
use actix::prelude::*;

struct MyActor {
    count: usize,
}

impl Actor for MyActor {
    type Context = Context<Self>;
}

# fn main() {}
```

Each actor has execution context, in this case we are going to use `Context<A>`. More information
on actor contexts is available in next section.

Now we need to define `Message` that actor needs to accept. Message could be any type 
that implement `Message` trait.

```rust
# extern crate actix;
use actix::prelude::*;

struct Ping(usize);

impl Message for Ping {
    type Result = usize;
}

# fn main() {}
```

Main purpose of the `Message` trait is to define result type. `Ping` message defines
`usize` result, which indicates that any actor that can accept `Ping` message needs to
return `usize` value.

And finally, we need to declare that our actor `MyActor` can accept `Ping` and handle it.
To do this, actor needs to implement `Handler<M>` trait.

Next, create an `Application` instance and register the
request handler with the application's `resource` on a particular *HTTP method* and *path*::

```rust
# extern crate actix;
# use actix::prelude::*;
#
# struct MyActor {
#    count: usize,
# }
# impl Actor for MyActor {
#     type Context = Context<Self>;
# }
#
# struct Ping(usize);
#
# impl Message for Ping {
#    type Result = usize;
# }

impl Handler<Ping> for MyActor {
    type Result = usize;
    
    fn handle(&mut self, msg: Ping, ctx: &mut Context<Self>) -> usize {
        self.count += msg.0;
        
        self.count
    }
}

# fn main() {}
```

That is it. Now we just need to start our actor and send message to it.
Start procedure depends on actor's context implementation. In our can we use
`Context<A>` which is tokio/future based. We can start it with `Actor::start()`
or `Actor::create()`. First is used if actor instance could be create immediately.
Second method is used in case if we need access to context object before we can create
actor instance. In case of `MyActor` actor we can use `start()` method.

All communications with actors go through an addres. You can `do_send` a message
without waiting for a response, or `send` an actor with specific message.
Both `start()` and `create()` methods returns address object.

In following example we are going to create `MyActor` actor and send one message.

```rust
# extern crate actix;
# extern crate futures;
# use futures::Future;
# use actix::prelude::*;
# struct MyActor {
#    count: usize,
# }
# impl Actor for MyActor {
#     type Context = Context<Self>;
# }
#
# struct Ping(usize);
#
# impl Message for Ping {
#    type Result = usize;
# }
# impl Handler<Ping> for MyActor {
#     type Result = usize;
#    
#     fn handle(&mut self, msg: Ping, ctx: &mut Context<Self>) -> usize {
#         self.count += msg.0;
#         self.count
#     }
# }
#
fn main() {
    let system = System::new("test");

    // start new actor
    let addr: Addr<Unsync, _> = MyActor{count: 10}.start();
    
    // send message and get future for result
    let res = addr.send(Ping(10));    
    
    Arbiter::handle().spawn(
        res.map(|res| {
            # Arbiter::system().do_send(actix::msgs::SystemExit(0));
            println!("RESULT: {}", res == 20);
        })
        .map_err(|_| ()));

    system.run();
}
```

Ping example is available in [examples directory](https://github.com/actix/actix/tree/master/examples/).
