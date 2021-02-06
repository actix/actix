# Actix Examples

The following examples complement the documentation and can help you understand how to use Actix:

1. [Ping](https://github.com/actix/actix/blob/master/examples/ping.rs) - Basic example showing 2 actors playing ping/pong.
2. [Fibonacci](https://github.com/actix/actix/blob/master/examples/fibonacci.rs) - Demonstrates how to integrate a synchronous actor on a threadpool for cpu bound tasks.
3. [Ring](https://github.com/actix/actix/blob/master/examples/ring.rs) - Ring benchmark inspired by Programming Erlang: Software for a Concurrent World. Send a M messages round a ring of N actors and benchmark.
4. [Chat](https://github.com/actix/actix/tree/master/examples/chat) - More realistic application example of a chat server/client

## External Examples

Nathan Hawkins has made an great demonstration on how to integrate a synchronous database in an asynchronous application using actix. Here's the [video of the demonstration](https://youtu.be/W-hvnVeRJzs) and the [code](https://github.com/utsl42/actix-test).
