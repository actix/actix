# Actix [![Build Status](https://travis-ci.org/fafhrd91/actix.svg?branch=master)](https://travis-ci.org/fafhrd91/actix)

Actix is a rust actor system framework.

* [API Documentation](http://fafhrd91.github.io/actix/actix/)
* Cargo package: [actix](https://crates.io/crates/actix)

---

Actix is licensed under the [Apache-2.0 license](http://opensource.org/licenses/APACHE-2.0).

## Usage

To use `actix`, add this to your `Cargo.toml`:

```toml
[dependencies]
actix = { git= "https://github.com/fafhrd91/actix.git" }
```


### Initiate the Actix

In order to use actix you first need to create an `System`.

```rust,ignore
extern crate actix;

fn main() {
    let system = actix::System::new("test".to_owned());
    
    system.run();
}
```

Actix uses [tokio](https://github.com/tokio-rs/tokio-core) event loop. 
`System::new()` call creates new event loop and starts `System` actor.
`system.run()` starts tokio event loop and will finish once the `System` actor 
receives `SystemExit` message.

Let's create simple Actor.

### Implement an Actor

In order to define an actor you need to define a struct and have it implement 
the [`Actor`](https://fafhrd91.github.io/actix/actix/trait.Actor.html) trait.


```rust
extern crate actix;
use actix::{Actor, ActorBuilder, Address, Arbiter, Context, System, SystemExit};

struct MyActor;

impl Actor for MyActor {
    fn started(&mut self, ctx: &mut Context<Self>) {
       println!("I am alive!");
       Arbiter::system().send(SystemExit(0));
    }
}

fn main() {
    let system = System::new("test".to_owned());

    let addr: Address<_> = MyActor.start();

    system.run();
    println!("Done");
}
```

Spawning a new actor is achieved via the methods of
[ActorBuilder](https://fafhrd91.github.io/actix/actix/trait.ActorBuilder.html) 
trait. It provides several different ways of creating actos, for details check docs. 
This trait is implemented for all actors. You can implement `started` and `finished`
mthods of Actor trait, `started` method get called when actor starts and 
`finished` when actor finishes.

### Handle messages

An Actor communicates with another Actor by sending an messages. In actix all messages are typed.
Let's define simple `Sum` message with two `usize` parameters and actor which will
accept this message and return sum of those two numbers.

```rust
extern crate actix;
extern crate futures;
use futures::{future, Future};
use actix::prelude::*;

// this is our Message
struct Sum(usize, usize);

// Actor definition
struct Summator;

impl Actor for Summator {}

// now we need to define `MessageHandler` for `Sum` message.
impl MessageHandler<Sum> for Summator {
    type Item = usize;
    type Error = ();
    type InputError = ();

    fn handle(&mut self, msg: Sum, ctx: &mut Context<Self>) -> MessageFuture<Self, Sum> {
        let sum = msg.0 + msg.1;
        sum.to_result()
    }
}


fn main() {
    let system = System::new("test".to_owned());

    let addr: Address<_> = Summator.start();

    // Address<A>::call() returns ActorFuture object, so we need to wait for result.
    // ActorFuture makes sense within Actor execution context, but we can use
    // Address<A>::call_fut() which return simple Future object.
    let res = addr.call_fut(Sum(10, 5));
    
    system.handle().spawn(res.then(|res| {
        match res {
            Ok(Ok(result)) => println!("SUM: {}", result),
            _ => println!("Something wrong"),
        }
        
        Arbiter::system().send(actix::SystemExit(0));
        future::result(Ok(()))
    }));

    system.run();
    println!("Done");
}
```

All communications with actors go through `Address` object. You can `send` message
without waiting response or `call` actor with specific message. `MessageHandler`
trait defines response type for message, `Item` and `Error` for value and error respectevily.
There are different types of addresses.
[`Address<A>`](https://fafhrd91.github.io/actix/actix/struct.Address.html) is address
of an actor that runs in same arbiter (event loop). If actor is running in different
thread [`SyncAddress<A>`](https://fafhrd91.github.io/actix/actix/struct.SyncAddress.html)
has to be used.

### Actor state and subscription for specific message

If you noticed methods of `Actor` and `MessageHandler` traits accept `&mut self`, so you are welcome to 
store anything in actor and mutate it whenever you need.

Address object requires actor type, but if we just want to send specific message to 
and actor that can handle message, we can use `Subscriber` interface. Let's create
new actor that uses `Subscriber`, also this example will show how to use standard future objects.

```rust
extern crate actix;
extern crate tokio_core;

use std::time::Duration;
use tokio_core::reactor::Timeout;

use actix::prelude::*;
use actix::actors::signal;

struct Ping;

// Actor definition
struct Game {
    counter: usize, 
    addr: Box<Subscriber<Ping>>
}

impl Actor for Game {}

// message handler for Ping message
impl MessageHandler<Ping> for Game {
    type Item = ();
    type Error = ();
    type InputError = ();

    fn handle(&mut self, msg: Ping, ctx: &mut Context<Self>) -> MessageFuture<Self, Ping> {
        self.counter += 1;
        
        if self.counter > 10 {
            Arbiter::system().send(actix::SystemExit(0));
        } else {
            println!("Ping received");
            
            // wait 100 nanos
            Timeout::new(Duration::new(0, 100), Arbiter::handle())
                .unwrap()
                .actfuture() // if we want get access to actor state we have to use ActorFuture
                .then(|_, srv: &mut Game, ctx: &mut Context<Self>| {
                     srv.addr.send(Ping);
                     fut::ok(())
                 })
                 .spawn(ctx);
        }
        ().to_result()
    }
}

fn main() {
    let system = System::new("test".to_owned());

    // this is helper actor that manages unix signals, by default stops System
    let _: () = signal::DefaultSignalsHandler.start();

    // we need Subscriber object so we need to use different builder method
    // which will allow to postpone actor creation
    let _: Address<_> = Game::create(|ctx| {
        // now we can get address of first actor and create second actor
        let addr: Address<_> = ctx.address();
        let addr2: Address<_> = Game{counter: 0, addr: addr.subscriber()}.start();
        
        // lets start pings
        addr2.send(Ping);
        
        // now we can finally create first actor
        Game{counter: 0, addr: addr2.subscriber()}
    });

    system.run();
}
```

More information on signals handling is in
[signal](https://fafhrd91.github.io/actix/actix/actors/signal/index.html) module.

### fectl

You may consider to check [fectl](https://github.com/fafhrd91/fectl) utility. It is written
with `actix` and shows how to create networking application with relatevly complex interactions.

## Contributing

All contribution are welcome, if you have a feature request don't hesitate to open an issue!
