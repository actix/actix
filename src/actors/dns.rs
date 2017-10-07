//! DNS lookup utils
//!
//!
//! ## Example
//!
//! ```rust
//! extern crate actix;
//! extern crate futures;
//! use futures::{future, Future};
//! use actix::prelude::*;
//! use actix::actors::dns;
//!
//! fn main() {
//!     let sys = System::new("test".to_owned());
//!
//!     let addr = SyncArbiter::start(3, || dns::DnsResolver);
//!
//!     Arbiter::handle().spawn(
//!        addr.call_fut(dns::GetAddressInfo::new(
//!            Some("127.0.0.1".to_owned()), None,
//!            dns::Family::Inet.to_int(), 0, dns::SocketType::Stream))
//!            .then(|res| {
//!                println!("RESULT: {:?}", res);
//!                Arbiter::system().send(msgs::SystemExit(0));
//!                future::result(Ok(()))
//!            }));
//!
//!     sys.run();
//! }
//! ```
use libc;
use std::mem;
use std::ffi::{CStr, CString, NulError};
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6, Ipv4Addr, Ipv6Addr};
use std::ptr;
use std::io;
use std::fmt;
use std::error::Error;
use std::os::raw::c_int;

use prelude::*;

/// DNS lookup actor
pub struct DnsResolver;

/// Request address info lookup message
pub struct GetAddressInfo {
    host: Option<String>,
    port: Option<String>,
    family: c_int,
    flags: c_int,
    socktype: SocketType,
}

impl Actor for DnsResolver {
    type Context = SyncContext<Self>;
}

impl GetAddressInfo {
    pub fn new(host: Option<String>, port: Option<String>,
               family: c_int, flags: c_int, socktype: SocketType) -> Self {
        GetAddressInfo {
            host: host,
            port: port,
            family: family,
            flags: flags,
            socktype: socktype}
    }
}

impl ResponseType<GetAddressInfo> for DnsResolver {
    type Item = Vec<AddrInfo>;
    type Error = LookupError;
}

impl Handler<GetAddressInfo> for DnsResolver {

    fn handle(&mut self, msg: GetAddressInfo, _: &mut SyncContext<Self>)
              -> Response<Self, GetAddressInfo>
    {
        match get_addrinfo(msg.host, msg.port, msg.family, msg.flags, msg.socktype) {
            Err(err) =>
                Self::reply_error(err),
            Ok(addrs) =>
                Self::reply(addrs.collect()),
        }
    }
}


pub const AI_PASSIVE: c_int = 0x0001;
pub const AI_CANONNAME: c_int = 0x0002;
pub const AI_NUMERICHOST: c_int = 0x0004;
pub const AI_NUMERICSERV: c_int = 0x0400;


#[derive(Copy, Clone, Debug)]
/// Address family
pub enum Family {
    /// Unspecified
    Unspec,
    /// Ipv4
    Inet,
    /// Ipv6
    Inet6,
    /// Unix domain soxket
    Unix,
    /// Some other
    Other(c_int),
}


impl Family {
    pub fn from_int(int: c_int) -> Self {
        match int {
            0 => Family::Unspec,
            libc::AF_INET => Family::Inet,
            libc::AF_INET6 => Family::Inet6,
            libc::AF_UNIX => Family::Unix,
            v => Family::Other(v),
        }
    }

    pub fn to_int(&self) -> c_int {
        match *self {
            Family::Unspec => 0,
            Family::Inet => libc::AF_INET,
            Family::Inet6 => libc::AF_INET6,
            Family::Unix => libc::AF_UNIX,
            Family::Other(v) => v,
        }
    }
}


#[derive(Copy, Clone, Debug)]
/// Types of Sockets
pub enum SocketType {
    /// Sequenced, reliable, connection-based byte streams.
    Stream,
    /// Connectionless, unreliable datagrams of fixed max length.
    DGram,
    /// Raw protocol interface.
    Raw,
    /// Some other
    Other(c_int),
}


impl SocketType {
    pub fn from_int(int: c_int) -> Self {
        match int {
            libc::SOCK_STREAM => SocketType::Stream,
            libc::SOCK_DGRAM => SocketType::DGram,
            libc::SOCK_RAW => SocketType::Raw,
            v => SocketType::Other(v),
        }
    }

    pub fn to_int(&self) -> c_int {
        match *self {
            SocketType::Stream => libc::SOCK_STREAM,
            SocketType::DGram => libc::SOCK_DGRAM,
            SocketType::Raw => libc::SOCK_RAW,
            SocketType::Other(v) => v,
        }
    }
}


#[derive(Copy, Clone, Debug)]
/// Socket Protocol
pub enum Protocol {
    /// Unspecificed.
    Unspec,
    /// Local to host (pipes and file-domain).
    Local,
    /// POSIX name for PF_LOCAL.
    Unix,
    /// IP Protocol Family.
    Inet,
    TCP,
    UDP,
    Other(c_int),
}


impl Protocol {
    pub fn from_int(int: c_int) -> Self {
        match int {
            0 => Protocol::Unspec,
            1 => Protocol::Local,
            2 => Protocol::Inet,
            6 => Protocol::TCP,
            17 => Protocol::UDP,
            v => Protocol::Other(v),
        }
    }

    #[cfg_attr(feature="cargo-clippy", allow(match_same_arms))]
    pub fn to_int(&self) -> c_int {
        match *self {
            Protocol::Unspec => 0,
            Protocol::Local => libc::PF_LOCAL,
            Protocol::Unix => libc::PF_UNIX,
            Protocol::Inet => libc::PF_INET,
            Protocol::TCP => 6,
            Protocol::UDP => 17,
            Protocol::Other(v) => v,
        }
    }
}


/// Resolved address information
#[derive(Clone, Debug)]
pub struct AddrInfo {
    pub flags: c_int,
    pub family: Family,
    pub socktype: SocketType,
    pub protocol: Protocol,
    pub sockaddr: SocketAddr,
    pub canonname: Option<String>,
}

impl AddrInfo {
    pub fn new(flags: c_int, family: Family,
               socktype: SocketType, protocol: Protocol,
               addr: SocketAddr, canonname: Option<String>) -> AddrInfo {
        AddrInfo {
            flags: flags,
            family: family,
            socktype: socktype,
            protocol: protocol,
            sockaddr: addr,
            canonname: canonname }
    }

    unsafe fn from_ptr(a: *mut libc::addrinfo) -> Result<Self, LookupError> {
        let addrinfo = *a;

        Ok(AddrInfo {
            flags: 0,
            family: Family::from_int(addrinfo.ai_family),
            socktype: SocketType::from_int(addrinfo.ai_socktype),
            protocol: Protocol::from_int(addrinfo.ai_protocol),
            sockaddr: sockaddr_to_addr(
                &*(addrinfo.ai_addr as *const libc::sockaddr_storage),
                addrinfo.ai_addrlen as usize)?,
            canonname: if addrinfo.ai_canonname.is_null() { None } else {
                Some(CStr::from_ptr(
                    addrinfo.ai_canonname).to_str().unwrap_or("unset").to_owned()) },
        })
    }
}


fn sockaddr_to_addr(storage: &libc::sockaddr_storage, len: usize) -> io::Result<SocketAddr> {
    match i32::from(storage.ss_family) {
        libc::AF_INET => {
            assert!(len as usize >= mem::size_of::<libc::sockaddr_in>());
            Ok(
                unsafe {
                    let sock = *(storage as *const _ as *const libc::sockaddr_in);
                    let ip = &*(&sock.sin_addr as *const libc::in_addr as *const Ipv4Addr);
                    SocketAddr::V4(SocketAddrV4::new(*ip, u16::from_be(sock.sin_port)))
                }
            )
        }
        libc::AF_INET6 => {
            assert!(len as usize >= mem::size_of::<libc::sockaddr_in6>());
            Ok(
                unsafe {
                    let sock = *(storage as *const _ as *const libc::sockaddr_in6);
                    let ip = &*(&sock.sin6_addr as *const libc::in6_addr as *const Ipv6Addr);
                    SocketAddr::V6(SocketAddrV6::new(
                        *ip, u16::from_be(sock.sin6_port),
                        u32::from_be(sock.sin6_flowinfo), 0))
                }
            )
        }
        _ => {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid argument"))
        }
    }
}

struct LookupAddrInfoInner {
    orig: *mut libc::addrinfo,
    cur: *mut libc::addrinfo,
}


/// Lookup a addr info via dns, return an iterator of addr infos.
fn get_addrinfo(host: Option<String>,
                port: Option<String>,
                family: c_int,
                flags: c_int,
                socktype: SocketType) -> Result<LookupAddrInfoInner, LookupError>
{
    let mut res = ptr::null_mut();
    let hints = libc::addrinfo {
        ai_flags: flags,
        ai_family: family,
        ai_socktype: socktype.to_int(),
        ai_protocol: 0,
        ai_addrlen: 0,
        ai_canonname: ptr::null_mut(),
        ai_addr: ptr::null_mut(),
        ai_next: ptr::null_mut(),
    };

    let tmp_h;
    let c_host = if let Some(host) = host {
        tmp_h = CString::new(host)?;
        tmp_h.as_ptr()
    } else {
        ptr::null()
    };

    let tmp_p;
    let c_srv = if let Some(port) = port {
        tmp_p = CString::new(port)?;
        tmp_p.as_ptr()
    } else {
        ptr::null()
    };

    unsafe {
        let lres = libc::getaddrinfo(c_host, c_srv, &hints, &mut res);
        match lres {
            0 => Ok(LookupAddrInfoInner { orig: res, cur: res }),
            _ => Err(LookupError::Generic),
        }
    }
}

impl Iterator for LookupAddrInfoInner {
    type Item = AddrInfo;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            loop {
                if self.cur.is_null() {
                    return None
                } else {
                    let ret = AddrInfo::from_ptr(self.cur);
                    self.cur = (*self.cur).ai_next as *mut libc::addrinfo;
                    if let Ok(ret) = ret {
                        return Some(ret)
                    }
                }
            }
        }
    }
}

impl Drop for LookupAddrInfoInner {
    fn drop(&mut self) { 
        unsafe { libc::freeaddrinfo(self.orig) }
    }
}


/// Errors that can occur looking up a hostname.
pub enum LookupError {
    /// A generic IO error
    IOError(io::Error),
    /// A Null Error
    NulError(NulError),
    /// Other error
    Other(String),
    /// An unspecific error
    Generic
}


impl From<io::Error> for LookupError {
    fn from(err: io::Error) -> Self {
        LookupError::IOError(err)
    }
}

impl From<NulError> for LookupError {
    fn from(err: NulError) -> Self {
        LookupError::NulError(err)
    }
}

impl<'a> From<&'a str> for LookupError {
    fn from(err: &'a str) -> Self {
        LookupError::Other(err.to_owned())
    }
}

impl Error for LookupError {
    fn description(&self) -> &str {
        match *self {
            LookupError::IOError(_) => "IO Error",
            LookupError::Other(ref err_str) => err_str,
            LookupError::NulError(_) => "nil pointer",
            LookupError::Generic => "generic error",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            LookupError::IOError(ref err) => Some(err),
            _ => None
        }
    }
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl fmt::Debug for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}
