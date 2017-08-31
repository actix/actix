use std;
use std::io;
use std::net::TcpListener;
use std::error::Error;
use std::os::unix::io::AsRawFd;

use serde_json as json;
use net2::TcpBuilder;
use net2::unix::UnixTcpBuilderExt;
use nix::fcntl::{fcntl, FcntlArg, FdFlag, FD_CLOEXEC};

use addrinfo;
use config::{Proto, SocketConfig};


pub struct Socket {
    pub name: String,
    pub listener: TcpListener,
    pub info: addrinfo::AddrInfo,
}


impl Socket {

    fn new(name: String, listener: TcpListener,
           info: addrinfo::AddrInfo, cfg: &SocketConfig) -> Socket {
        let fd = listener.as_raw_fd();
        std::env::set_var(format!("FECTL_FD_{}", name),
                          format!("{},FAMILY:{},SOCKETTYPE:{},PROTO:{}",
                                  fd.to_string(),
                                  info.family.to_int(),
                                  info.socktype.to_int(),
                                  info.protocol.to_int()));
        // loader
        if let Some(ref app) = cfg.app {
            std::env::set_var(format!("FECTL_APP_{}", name), app);

            // encode arguments
            if !cfg.arguments.is_empty() {
                let args = json::to_string(&cfg.arguments).unwrap();
                std::env::set_var(format!("FECTL_ARGS_{}", name), args);
            }
        }

        let mut flags = FdFlag::from_bits_truncate(fcntl(fd, FcntlArg::F_GETFD).unwrap());
        flags.remove(FD_CLOEXEC);
        let _ = fcntl(fd, FcntlArg::F_SETFD(flags));

        Socket {
            name: name.clone(),
            listener: listener,
            info: info,
        }
    }

    pub fn load_config(cfg: &Vec<SocketConfig>) -> Result<Vec<Socket>, std::io::Error>
    {
        let mut services = Vec::new();

        for sock in cfg.iter() {
            // resolve addresses
            let lookup = addrinfo::lookup_addrinfo(
                sock.host.clone(), Some(sock.port.to_string()), 0,
                addrinfo::AI_PASSIVE, addrinfo::SocketType::Stream)?;
            let addrs: Vec<addrinfo::AddrInfo> = lookup.collect();
            if addrs.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::Other, "getaddrinfo() returned empty list"))
            }

            // start listen
            let mut found = false;
            for addr in addrs {
                let builder = match addr.family {
                    addrinfo::Family::Inet => {
                        if sock.proto == Proto::tcp6 {
                            continue
                        }
                        if let Ok(b) = TcpBuilder::new_v4() {
                            b
                        } else {
                            continue
                        }
                    }
                    addrinfo::Family::Inet6 => {
                        if sock.proto == Proto::tcp4 {
                            continue
                        }
                        if let Ok(b) = TcpBuilder::new_v6() {
                            let _ = b.only_v6(true);
                            b
                        } else {
                            continue
                        }
                    },
                    _ => continue
                };

                let _ = builder.reuse_address(true);
                let _ = builder.reuse_port(true);

                match builder.bind(addr.sockaddr) {
                    Ok(_) => {
                        if let Ok(lst) = builder.listen(sock.backlog as i32) {
                            info!("Init listener on {:?}", addr.sockaddr);
                            let mut addr = addr.clone();
                            addr.sockaddr = lst.local_addr().expect("should not fail");
                            services.push(Socket::new(sock.name.clone(), lst, addr, sock));
                            found = true;
                            break;
                        }
                    },
                    Err(err) => {
                        println!("Can not bind to address: \"{}\" {:?}",
                                 addr.sockaddr, err.description());
                    }
                }
            }
            if !found {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Can not start listener for `{}` service", sock.name)))
            }
        }
        Ok(services)
    }
}


impl Drop for Socket {

    fn drop(&mut self) {
        std::env::remove_var(format!("FD_{}", self.name));
    }
}
