//! DNS resolver and connector utility actor
//!
//! ## Example
//!
//! ```rust
//! #![recursion_limit="128"]
//! use futures::{future, FutureExt};
//! use actix::prelude::*;
//! use actix::actors::resolver;
//!
//! #[actix_rt::main]
//! async fn main() {
//!         actix_rt::spawn(async {
//!             let resolver = resolver::Resolver::from_registry();
//!
//!             let addrs = resolver.send(
//!                  resolver::Connect::host("localhost")).await;
//!
//!             println!("RESULT: {:?}", addrs);
//!             System::current().stop();
//!        });
//!
//!         actix_rt::spawn(async {
//!             let resolver = resolver::Resolver::from_registry();
//!
//!             let stream = resolver.send(
//!                  resolver::Connect::host_and_port("localhost", 5000)).await;
//!
//!             println!("RESULT: {:?}", stream);
//!        });
//! }
//! ```
use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{self, Poll};
use std::time::Duration;

use derive_more::Display;
use log::warn;
use tokio::net::TcpStream;
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::AsyncResolver;
use trust_dns_resolver::BackgroundLookupIp;

use crate::clock::Delay;
use crate::fut::wrap_future;
use crate::fut::ActorFuture;
use crate::fut::Either;
use crate::prelude::*;

#[deprecated(since = "0.7.0", note = "please use `Resolver` instead")]
pub type Connector = Resolver;

#[deprecated(since = "0.7.0", note = "please use `ResolverError` instead")]
pub type ConnectorError = ResolverError;

#[derive(Eq, PartialEq, Debug)]
pub struct Resolve {
    pub name: String,
    pub port: Option<u16>,
}

impl Resolve {
    pub fn host<T: AsRef<str>>(host: T) -> Resolve {
        Resolve {
            name: host.as_ref().to_owned(),
            port: None,
        }
    }

    pub fn host_and_port<T: AsRef<str>>(host: T, port: u16) -> Resolve {
        Resolve {
            name: host.as_ref().to_owned(),
            port: Some(port),
        }
    }
}

impl Message for Resolve {
    type Result = Result<VecDeque<SocketAddr>, ResolverError>;
}

#[derive(Eq, PartialEq, Debug)]
pub struct Connect {
    pub name: String,
    pub port: Option<u16>,
    pub timeout: Duration,
}

impl Connect {
    pub fn host<T: AsRef<str>>(host: T) -> Connect {
        Connect {
            name: host.as_ref().to_owned(),
            port: None,
            timeout: Duration::from_secs(1),
        }
    }

    pub fn host_and_port<T: AsRef<str>>(host: T, port: u16) -> Connect {
        Connect {
            name: host.as_ref().to_owned(),
            port: Some(port),
            timeout: Duration::from_secs(1),
        }
    }

    /// Set connect timeout
    ///
    /// By default timeout is set to a 1 second.
    pub fn timeout(mut self, timeout: Duration) -> Connect {
        self.timeout = timeout;
        self
    }
}

impl Message for Connect {
    type Result = Result<TcpStream, ResolverError>;
}

#[derive(Eq, PartialEq, Debug)]
pub struct ConnectAddr(pub SocketAddr);

impl Message for ConnectAddr {
    type Result = Result<TcpStream, ResolverError>;
}

#[derive(Debug, Display)]
pub enum ResolverError {
    /// Failed to resolve the hostname
    #[display(fmt = "Failed resolving hostname: {}", _0)]
    Resolver(String),

    /// Address is invalid
    #[display(fmt = "Invalid input: {}", _0)]
    InvalidInput(&'static str),

    /// Connecting took too long
    #[display(fmt = "Timeout out while establishing connection")]
    Timeout,

    /// Connection io error
    #[display(fmt = "{}", _0)]
    IoError(io::Error),
}

/// `InternalServerError` for `actix::MailboxError`
#[cfg(feature = "http")]
impl actix_http::ResponseError for ResolverError {}

pub struct Resolver {
    resolver: Option<AsyncResolver>,
    cfg: Option<(ResolverConfig, ResolverOpts)>,
    err: Option<String>,
}

impl Resolver {
    pub fn new(config: ResolverConfig, options: ResolverOpts) -> Resolver {
        Resolver {
            resolver: None,
            cfg: Some((config, options)),
            err: None,
        }
    }

    fn start_resolver<F>(
        &self,
        ctx: &mut <Self as Actor>::Context,
        parts: (AsyncResolver, F),
    ) -> AsyncResolver
    where
        F: 'static + Future<Output = ()>,
    {
        ctx.spawn(wrap_future::<_, Self>(parts.1));
        parts.0
    }
}

impl Actor for Resolver {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // AsyncResolver::new() returns the AsyncResolver itself, plus an anonymous
        // future which gets spawned as a background task for doing DNS
        // resolution. So we use our little `start_resolver` wrapper to spawn
        // the background task (which gets cleaned up automatically if no
        // outstanding AsyncResolvers still have a handle to it).
        let resolver = if let Some(cfg) = self.cfg.take() {
            self.start_resolver(ctx, AsyncResolver::new(cfg.0, cfg.1))
        } else {
            match AsyncResolver::from_system_conf() {
                Ok(resolver) => self.start_resolver(ctx, resolver),
                Err(err) => {
                    warn!("Can not create system dns resolver: {}", err);
                    self.start_resolver(
                        ctx,
                        AsyncResolver::new(
                            ResolverConfig::default(),
                            ResolverOpts::default(),
                        ),
                    )
                }
            }
        };

        // Keep the resolver itself.
        self.resolver = Some(resolver);
    }
}

impl Supervised for Resolver {}

impl SystemService for Resolver {}

impl Default for Resolver {
    fn default() -> Resolver {
        Resolver {
            resolver: None,
            cfg: None,
            err: None,
        }
    }
}

impl Handler<Resolve> for Resolver {
    type Result = ResponseActFuture<Self, Result<VecDeque<SocketAddr>, ResolverError>>;

    fn handle(&mut self, msg: Resolve, _: &mut Self::Context) -> Self::Result {
        if let Some(ref err) = self.err {
            Box::new(ResolveFut::err(err.clone()))
        } else {
            Box::new(ResolveFut::new(
                msg.name,
                msg.port.unwrap_or(0),
                self.resolver.as_ref().unwrap(),
            ))
        }
    }
}

impl Handler<Connect> for Resolver {
    type Result = ResponseActFuture<Self, Result<TcpStream, ResolverError>>;

    fn handle(&mut self, msg: Connect, _: &mut Self::Context) -> Self::Result {
        let timeout = msg.timeout;
        Box::new(
            ResolveFut::new(
                msg.name,
                msg.port.unwrap_or(0),
                self.resolver.as_ref().unwrap(),
            )
            .then(move |addrs, act, _| match addrs {
                Ok(a) => Either::Left(TcpConnector::with_timeout(a, timeout)),
                Err(e) => Either::Right(async move { Err(e) }.into_actor(act)),
            }),
        )
    }
}

impl Handler<ConnectAddr> for Resolver {
    type Result = ResponseActFuture<Self, Result<TcpStream, ResolverError>>;

    fn handle(&mut self, msg: ConnectAddr, _: &mut Self::Context) -> Self::Result {
        let mut v = VecDeque::new();
        v.push_back(msg.0);
        Box::new(TcpConnector::new(v))
    }
}

/// A resolver future.
struct ResolveFut {
    lookup: Option<BackgroundLookupIp>,
    port: u16,
    addrs: Option<VecDeque<SocketAddr>>,
    error: Option<ResolverError>,
    error2: Option<String>,
}

impl ResolveFut {
    pub fn new<S: AsRef<str>>(
        addr: S,
        port: u16,
        resolver: &AsyncResolver,
    ) -> ResolveFut {
        // try to parse as a regular SocketAddr first
        if let Ok(addr) = addr.as_ref().parse() {
            let mut addrs = VecDeque::new();
            addrs.push_back(addr);

            ResolveFut {
                port,
                lookup: None,
                addrs: Some(addrs),
                error: None,
                error2: None,
            }
        } else {
            // we need to do dns resolution
            match ResolveFut::parse(addr.as_ref(), port) {
                Ok((host, port)) => ResolveFut {
                    port,
                    lookup: Some(resolver.lookup_ip(host)),
                    addrs: None,
                    error: None,
                    error2: None,
                },
                Err(err) => ResolveFut {
                    port,
                    lookup: None,
                    addrs: None,
                    error: Some(err),
                    error2: None,
                },
            }
        }
    }

    pub fn err(err: String) -> ResolveFut {
        ResolveFut {
            port: 0,
            lookup: None,
            addrs: None,
            error: None,
            error2: Some(err),
        }
    }

    fn parse(addr: &str, port: u16) -> Result<(&str, u16), ResolverError> {
        macro_rules! try_opt {
            ($e:expr, $msg:expr) => {
                match $e {
                    Some(r) => r,
                    None => return Err(ResolverError::InvalidInput($msg)),
                }
            };
        }

        // split the string by ':' and convert the second part to u16
        let mut parts_iter = addr.splitn(2, ':');
        let host = try_opt!(parts_iter.next(), "invalid socket address");
        let port_str = parts_iter.next().unwrap_or("");
        let port: u16 = port_str.parse().unwrap_or(port);

        Ok((host, port))
    }
}

impl ActorFuture for ResolveFut {
    type Output = Result<VecDeque<SocketAddr>, ResolverError>;
    type Actor = Resolver;
    fn poll(
        mut self: Pin<&mut Self>,
        _: &mut Resolver,
        _: &mut Context<Resolver>,
        task: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        if let Some(err) = self.error.take() {
            Poll::Ready(Err(err))
        } else if let Some(err) = self.error2.take() {
            Poll::Ready(Err(ResolverError::Resolver(err)))
        } else if let Some(addrs) = self.addrs.take() {
            Poll::Ready(Ok(addrs))
        } else {
            match Pin::new(self.lookup.as_mut().unwrap()).poll(task) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok(ips)) => {
                    let addrs: VecDeque<_> = ips
                        .iter()
                        .map(|ip| SocketAddr::new(ip, self.port))
                        .collect();
                    if addrs.is_empty() {
                        Poll::Ready(Err(ResolverError::Resolver(
                            "Expect at least one A dns record".to_owned(),
                        )))
                    } else {
                        Poll::Ready(Ok(addrs))
                    }
                }
                Poll::Ready(Err(err)) => {
                    Poll::Ready(Err(ResolverError::Resolver(format!("{}", err))))
                }
            }
        }
    }
}

/// A TCP stream connector.
#[allow(clippy::type_complexity)]
pub struct TcpConnector {
    addrs: VecDeque<SocketAddr>,
    timeout: Delay,
    stream: Option<Pin<Box<dyn Future<Output = Result<TcpStream, io::Error>>>>>,
}

impl TcpConnector {
    pub fn new(addrs: VecDeque<SocketAddr>) -> TcpConnector {
        TcpConnector::with_timeout(addrs, Duration::from_secs(1))
    }

    pub fn with_timeout(addrs: VecDeque<SocketAddr>, timeout: Duration) -> TcpConnector {
        TcpConnector {
            addrs,
            stream: None,
            timeout: tokio::time::delay_for(timeout),
        }
    }
}

impl ActorFuture for TcpConnector {
    type Output = Result<TcpStream, ResolverError>;
    type Actor = Resolver;

    fn poll(
        self: Pin<&mut Self>,
        _: &mut Resolver,
        _: &mut Context<Resolver>,
        cx: &mut task::Context<'_>,
    ) -> Poll<Self::Output> {
        let mut this = self.get_mut();

        // timeout
        if let Poll::Ready(_) = Pin::new(&mut this.timeout).poll(cx) {
            return Poll::Ready(Err(ResolverError::Timeout));
        }

        // connect
        loop {
            if let Some(ref mut fut) = this.stream {
                match Pin::new(fut).poll(cx) {
                    Poll::Ready(Ok(sock)) => return Poll::Ready(Ok(sock)),
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => {
                        if this.addrs.is_empty() {
                            return Poll::Ready(Err(ResolverError::IoError(err)));
                        }
                    }
                }
            }
            // try to connect
            let addr = this.addrs.pop_front().unwrap();
            this.stream = Some(Box::pin(TcpStream::connect(addr)));
        }
    }
}
