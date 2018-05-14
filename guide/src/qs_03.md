# Actor

Actix is a rust library providing a framework for developing concurrent applications.

Actix is built on the [Actor Model](https://en.wikipedia.org/wiki/Actor_model) which
allows applications to be written as a group of independently executing but cooperating
"Actors" which communicate via messages. Actors are objects which encapsulate
state and behavior and run within the *Actor System* provided by the actix library.

Actors run within a specific execution context [*Context<A>*](../actix/struct.Context.html).
The context object is available only during execution. Each actor has a separate
execution context. The execution context also controls the lifecycle of an actor.

Actors communicate exclusively by exchanging messages. The sending actor can
wait for the response. Actors are not referenced directly, but by different
types of addresses: non thread safe [*Addr<Unsync, A>*](../actix/struct.Addr.html) or
thread safe [*Addr<Syn, A>*](../actix/struct.Syn.html)

Any rust type can be an actor, it only needs to implement the [*Actor*](../actix/trait.Actor.html) trait.

To be able to handle a specific message the actor has to provide a
[*Handler<M>*](../actix/trait.Handler.html) implementation for this message. All messages
are statically typed. The message can be handled in an asynchronous fashion.
Actor can spawn other actors or add futures or streams to execution context.
The `Actor` trait provides several methods that allow controlling the actor's lifecycle.


## Actor lifecycle

### Started

An actor always starts in the `Started` state. During this state the actor's `started()`
method is called. The `Actor` trait provides a default implementation for this method.
The actor context is available during this state and the actor can start more actors or register
async streams or do any other required configuration.

### Running

After an Actor's `started()` method is called, the actor transitions to the `Running` state.
The Actor can stay in `running` state indefinitely long.

### Stopping

The Actor's execution state changes to the `stopping` state in the following situations:

* `Context::stop` is called by the actor itself
* all addresses to the actor get dropped. i.e. no other actor references it.
* no event objects are registered in the context.

An actor can restore from the `stopping` state to the `running` state by creating a new
address or adding an event object, and by returning `Running::Continue`.

If an actor changed state to `stopping` because `Context::stop()` is called
then the context immediately stops processing incoming messages and calls
`Actor::stopping()`. If the actor does not restore back to the `running` state, all
unprocessed messages are dropped.

By default this method returns `Running::Stop` which confirms the stop operation.

### Stopped

If the actor does not modify the execution context during the stopping state, the actor state changes
to `Stopped`. This state is considered final and at this point the actor is dropped.


## Message

An Actor communicates with other actors by sending messages. In actix all
messages are typed. A message can be any rust type which implements the
[Message](../actix/trait.Message.html) trait. *Message::Result* defines the return type.
Let's define a simple `Ping` message - an actor which will accept this message needs to return
`io::Result<bool>`.

```rust
# extern crate actix;
use std::io;
use actix::prelude::*;

struct Ping;

impl Message for Ping {
    type Result = Result<bool, io::Error>;
}

# fn main() {}
```

## Spawning an actor

How to start an actor depends on its context. Spawning a new async actor
is achieved via the `start` and `create` methods of
the [Actor](../actix/trait.Actor.html) trait. It provides several different ways of
creating actors; for details check the docs.

## Complete example

```rust
# extern crate actix;
# extern crate futures;
use std::io;
use actix::prelude::*;
use futures::Future;

/// Define message
struct Ping;

impl Message for Ping {
    type Result = Result<bool, io::Error>;
}


// Define actor
struct MyActor;

// Provide Actor implementation for our actor
impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
       println!("Actor is alive");
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
       println!("Actor is stopped");
    }
}

/// Define handler for `Ping` message
impl Handler<Ping> for MyActor {
    type Result = Result<bool, io::Error>;

    fn handle(&mut self, msg: Ping, ctx: &mut Context<Self>) -> Self::Result {
        println!("Ping received");

        Ok(true)
    }
}

fn main() {
    let sys = System::new("example");

    // Start MyActor in current thread
    let addr: Addr<Unsync, _> = MyActor.start();

    // Send Ping message.
    // send() message returns Future object, that resolves to message result
    let result = addr.send(Ping);

    // spawn future to reactor
    Arbiter::handle().spawn(
        result.map(|res| {
            match res {
                Ok(result) => println!("Got result: {}", result),
                Err(err) => println!("Got error: {}", err),
            }
#           Arbiter::system().do_send(actix::msgs::SystemExit(0));
        })
        .map_err(|e| {
            println!("Actor is probably died: {}", e);
        }));

    sys.run();
}
```

## Responding with a MessageResponse

Let's take a look at the `Result` type defined for the `impl Handler` in the above example. See how we're returning a `Result<bool, io::Error>`? We're able to respond to our actor's incoming message with this type because it has the `MessageResponse` trait implemented for that type. Here's the definition for that trait:

```
pub trait MessageResponse<A: Actor, M: Message> {
    fn handle<R: ResponseChannel<M>>(self, ctx: &mut A::Context, tx: Option<R>);
}
```

Sometimes it makes sense to respond to incoming messages with types that don't have this trait implemented for them. When that happens we can implement the trait ourselves. Here's an example where we're responding to a `Ping` message with a `GotPing`, and responding with `GotPong` for a `Pong` message.

```rust
# extern crate actix;
# extern crate futures;
use actix::dev::{MessageResponse, ResponseChannel};
use actix::prelude::*;
use futures::Future;

enum Messages {
    Ping,
    Pong,
}

enum Responses {
    GotPing,
    GotPong,
}

impl<A, M> MessageResponse<A, M> for Responses
where
    A: Actor,
    M: Message<Result = Responses>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

impl Message for Messages {
    type Result = Responses;
}

// Define actor
struct MyActor;

// Provide Actor implementation for our actor
impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("Actor is alive");
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        println!("Actor is stopped");
    }
}

/// Define handler for `Messages` enum
impl Handler<Messages> for MyActor {
    type Result = Responses;

    fn handle(&mut self, msg: Messages, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            Messages::Ping => Responses::GotPing,
            Messages::Pong => Responses::GotPong,
        }
    }
}

fn main() {
    let sys = System::new("example");

    // Start MyActor in current thread
    let addr: Addr<Unsync, _> = MyActor.start();

    // Send Ping message.
    // send() message returns Future object, that resolves to message result
    let ping_future = addr.send(Messages::Ping);
    let pong_future = addr.send(Messages::Pong);

    // Get handle to Arbiter's reactor
    let handle = Arbiter::handle();

    // Spawn pong_future onto event loop
    handle.spawn(
        pong_future
            .map(|res| {
                match res {
                    Responses::GotPing => println!("Ping received"),
                    Responses::GotPong => println!("Pong received"),
                }
#               Arbiter::system().do_send(actix::msgs::SystemExit(0));
            })
            .map_err(|e| {
                println!("Actor is probably died: {}", e);
            }),
    );

    // Spawn ping_future onto event loop
    handle.spawn(
        ping_future
            .map(|res| {
                match res {
                    Responses::GotPing => println!("Ping received"),
                    Responses::GotPong => println!("Pong received"),
                }
#               Arbiter::system().do_send(actix::msgs::SystemExit(0));
            })
            .map_err(|e| {
                println!("Actor is probably died: {}", e);
            }),
    );

    sys.run();
}
```
