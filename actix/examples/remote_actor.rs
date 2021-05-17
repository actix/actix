//! Example for two actors communicate through Tcp.
//! With one actor send message and the other handle.

use std::{error, io, rc::Rc};

use actix::prelude::*;
use serde_derive::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::wrappers::TcpListenerStream;

type Error = Box<dyn error::Error + Send + Sync + 'static>;

/// An actor message type from local actor to remote.
#[derive(Debug, Deserialize, Serialize)]
struct Ask {
    ask: String,
}

impl Message for Ask {
    type Result = Result<Answer, Error>;
}

/// An actor message type from remote actor to local.
#[derive(Debug, Deserialize, Serialize)]
struct Answer {
    answer: String,
}

impl Message for Answer {
    type Result = ();
}

#[actix::main]
async fn main() -> Result<(), Error> {
    let lst = TcpListener::bind("127.0.0.1:0").await?;
    let socket_addr = lst.local_addr()?;

    let _remote_addr = RemoteActor::create(move |ctx| {
        let stream = TcpListenerStream::new(lst);
        // add tcp listener stream to actor context.
        ctx.add_stream(stream);

        println!("Started remote actor on {:?}", socket_addr);

        RemoteActor
    });

    let stream = TcpStream::connect(socket_addr).await?;
    let socket_addr = stream.local_addr()?;

    let local_addr = LocalActor::create(|_| {
        println!("Started local actor on {:?}", socket_addr);
        LocalActor(Rc::new(stream))
    });

    let ask = Ask { ask: String::from("Can I have this feature?")};

    let answer = local_addr.send(ask).await??;

    // use the same local actor to handle answer message but this can be dispatched
    // to other actors on the same process.
    local_addr.send(answer).await?;

    Ok(())
}

struct RemoteActor;

impl Actor for RemoteActor {
    type Context = Context<Self>;
}

/// TcpListenerStream produces io::Result<TcpStream>) so the StreamHandler would
/// handle the same type.
impl StreamHandler<io::Result<TcpStream>> for RemoteActor {
    fn handle(&mut self, res: io::Result<TcpStream>, ctx: &mut Self::Context) {
        match res {
            Ok(stream) => {
                // handle incoming tcp connection.
                async move {
                    let addr = stream.peer_addr()?;
                    println!("Actor from {:?} connected", addr);

                    // For example usage a fix sized buffer is used.
                    let mut buf = [0u8; 1024];

                    // async tcp read/write.
                    loop {
                        stream.readable().await?;

                        match stream.try_read(&mut buf) {
                            // on success try to deserialize read data to message.
                            Ok(n) => match bincode::deserialize::<Ask>(&buf) {
                                // deserialzie success. return both stream and message.
                                Ok(msg) => return Ok::<_, Error>((stream, msg)),
                                Err(e) if n == 0 => return Err(e.into()),
                                _ => continue,
                            },
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                            Err(e) => return Err(e.into()),
                        };
                    }
                }
                // transform async/await to actor future.
                .into_actor(self)
                // handle the output of async/await. _act and _ctx are Actor's state and context.
                // This gives the ability to mutate actor state and/or operate on context
                // based on the output of async/await
                .and_then(|(stream, ask), act, ctx| {
                    let addr = ctx.address();
                    // and_then combinator would ask for another actor future that share the same
                    // result type as previous actor future(async/await block).
                    async move {
                        // Ask message is sent to self but it can be any actors runs on reomte
                        // actor's process.
                        let answer = addr.send(ask).await??;
                        // serialize answer.
                        let buf = bincode::serialize(&answer)?;

                        // async write answer to local actor.
                        let mut written = 0;
                        let total = buf.len();
                        while written < total {
                            stream.writable().await?;
                            match stream.try_write(&buf[written..]) {
                                Ok(n) => written += n,
                                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                                Err(e) => return Err(e.into()),
                            }
                        }

                        Ok(())
                    }
                    .into_actor(act)
                })
                // map combinator function similar to and_then but it gives the complete output
                // of previous chained actor futures.
                // It would ask for another output type of future. In this case it's () as
                // spawned actor future tasks must output () at last.
                .map(|res, _act, _ctx| {
                    if let Err(e) = res {
                        println!("{:?}", e)
                    }
                })
                // spawn task and handle next TcpStream.
                // task would still running till finish in a non blocking manner.
                .spawn(ctx);
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::ConnectionReset
                    || e.kind() == io::ErrorKind::ConnectionAborted
                    || e.kind() == io::ErrorKind::ConnectionRefused =>
            {
                // ignore non fatal error.
            }
            Err(ref e) => {
                println!("fatal error {:?}", e);
            }
        }
    }
}

/// Remote actor handling for Ask Message.
impl Handler<Ask> for RemoteActor {
    type Result = Response<Result<Answer, Error>>;

    fn handle(&mut self, msg: Ask, _: &mut Self::Context) -> Self::Result {
        println!("Question is: {}", msg.ask);

        Response::reply(Ok(Answer {
            answer: String::from("No idea!"),
        }))
    }
}

struct LocalActor(Rc<TcpStream>);

impl Actor for LocalActor {
    type Context = Context<Self>;
}

/// Ask is the message sent to Remote actor.
/// Use Tcp connection to send it and wait for Answer message back.
impl Handler<Ask> for LocalActor {
    type Result = ResponseActFuture<Self, Result<Answer, Error>>;

    fn handle(&mut self, msg: Ask, _: &mut Self::Context) -> Self::Result {
        // clone the tcp stream so it can be used in async tasks.
        // This would enable non blocking IO on multiple messages as they can
        // use read or write part of the IO separately and not blocking one another.
        let stream = self.0.clone();

        Box::pin(
            async move {
                // serialize ask message.
                let buf = bincode::serialize(&msg)?;

                // async write ask to remote actor.
                let mut written = 0;
                let total = buf.len();
                while written < total {
                    stream.writable().await?;
                    match stream.try_write(&buf[written..]) {
                        Ok(n) => written += n,
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                        Err(e) => return Err(e.into()),
                    }
                }

                // async read until answer message is back.
                let mut buf = [0u8; 1024];
                loop {
                    stream.readable().await?;
                    match stream.try_read(&mut buf) {
                        Ok(n) => match bincode::deserialize::<Answer>(&buf) {
                            Ok(msg) => return Ok(msg),
                            Err(e) if n == 0 => return Err(e.into()),
                            Err(_) => continue,
                        },
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            .into_actor(self),
        )
    }
}

/// Answer if the message sent back from Remote actor.
/// just print it.
impl Handler<Answer> for LocalActor {
    type Result = ();

    fn handle(&mut self, msg: Answer, _: &mut Self::Context) -> Self::Result {
        println!("Answer is: {}", msg.answer);
    }
}
