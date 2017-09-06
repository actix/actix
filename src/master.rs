use std;
use std::io;
use std::rc::Rc;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::time::Duration;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixListener as StdUnixListener;

use nix;
use libc;
use serde_json as json;
use byteorder::{BigEndian , ByteOrder};
use bytes::{BytesMut, BufMut};
use futures::unsync;
use futures::Async;
use tokio_core::reactor;
use tokio_core::reactor::Timeout;
use tokio_uds::{UnixStream, UnixListener};
use tokio_io::AsyncRead;
use tokio_io::codec::{Encoder, Decoder};

use ctx::prelude::*;

use client;
use logging;
use config::Config;
use version::PKG_INFO;
use cmd::{CommandCenter, CommandError};
use service::{StartStatus, ReloadStatus, ServiceOperationError};

/// Master command
#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag="cmd", content="data")]
pub enum MasterRequest {
    /// Ping master process
    Ping,
    /// Status
    Status(String),
    /// Start service
    Start(String),
    /// Pause service
    Pause(String),
    /// Resume service
    Resume(String),
    /// Gracefully reload service
    Reload(String),
    /// Restart service
    Restart(String),
    /// Gracefully stop service
    Stop(String),
    /// Pid of the master process
    Pid,
    /// Quit process
    Quit,
    /// Version if the master
    Version,
}

/// Master responses
#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag="cmd", content="data")]
pub enum MasterResponse {
    Pong,
    Done,
    /// Pid of the master process
    Pid(String),
    /// Version of the master process
    Version(String),

    /// Service started
    ServiceStarted,
    /// Service Stopped
    ServiceStopped,
    /// Service failed, service is not available
    ServiceFailed,
    /// Service status
    ServiceStatus(String),

    /// System not ready
    ErrorNotReady,
    /// Service is unknown
    ErrorUnknownService,
    /// Service is starting
    ErrorServiceStarting,
    /// Service is running
    ErrorServiceRunning,
    /// Service is reloading
    ErrorServiceReloading,
    /// Service is stopping
    ErrorServiceStopping,
    /// Service is stopped
    ErrorServiceStopped,
    /// Service is failed
    ErrorServiceFailed,
}

pub struct Master {
    cfg: Rc<Config>,
    cmd: Rc<RefCell<CommandCenter>>,
}

impl Master {

    pub fn start(cfg: Config, lst: StdUnixListener) -> bool {
        let cfg = Rc::new(cfg);

        // create core
        let mut core = reactor::Core::new().unwrap();
        let handle = core.handle();

        // create uds stream
        let lst = match UnixListener::from_listener(lst, &handle) {
            Ok(lst) => lst,
            Err(err) => {
                error!("Can not create unix socket listener {:?}", err);
                return false
            }
        };

        // command center
        let (stop_tx, stop_rx) = unsync::oneshot::channel();
        let cmd = CommandCenter::new(cfg.clone(), &handle, stop_tx);

        // start uds master server
        let master = Master {
            cfg: cfg,
            cmd: cmd,
        };
        MasterListener.run(master, lst.incoming(), &handle);

        // run loop
        match core.run(stop_rx) {
            Ok(success) => success,
            Err(_) => false,
        }
    }

    fn hb(&self, srv: &mut CtxFramedService<MasterClient>) {
        let fut = Timeout::new(Duration::new(1, 0), srv.handle())
            .unwrap()
            .wrap()
            .then(|_, ctx: &mut Master, srv: &mut CtxFramedService<MasterClient>| {
                srv.send_buffered(MasterResponse::Pong);
                ctx.hb(srv);
                fut::ok(())
            });
        srv.spawn(fut);
    }
}

impl Drop for Master {
    fn drop(&mut self) {
        self.cfg.master.remove_files();
    }
}

struct MasterClient;

#[derive(Debug)]
enum MasterClientMessage {
    Request(MasterRequest),
}

impl MasterClient {
    fn handle_error(&self, err: CommandError, srv: &mut CtxFramedService<Self>) {
        match err {
            CommandError::NotReady =>
                srv.send_buffered(MasterResponse::ErrorNotReady),
            CommandError::UnknownService =>
                srv.send_buffered(MasterResponse::ErrorUnknownService),
            CommandError::ServiceStopped =>
                srv.send_buffered(MasterResponse::ErrorServiceStopped),
            CommandError::Service(err) => match err {
                ServiceOperationError::Starting =>
                    srv.send_buffered(MasterResponse::ErrorServiceStarting),
                ServiceOperationError::Reloading =>
                    srv.send_buffered(MasterResponse::ErrorServiceReloading),
                ServiceOperationError::Stopping =>
                    srv.send_buffered(MasterResponse::ErrorServiceStopping),
                ServiceOperationError::Running =>
                    srv.send_buffered(MasterResponse::ErrorServiceRunning),
                ServiceOperationError::Stopped =>
                    srv.send_buffered(MasterResponse::ErrorServiceStopped),
                ServiceOperationError::Failed =>
                    srv.send_buffered(MasterResponse::ErrorServiceFailed),
            }
        }
    }

    fn stop(&self, name: String, ctx: &mut Master, srv: &mut CtxFramedService<Self>) {
        info!("Client command: Stop service '{}'", name);
        match ctx.cmd.borrow_mut().stop_service(name.as_str(), true) {
            Err(err) => match err {
                CommandError::ServiceStopped =>
                    srv.send_buffered(MasterResponse::ServiceStarted),
                _ => self.handle_error(err, srv),
            }
            Ok(rx) => {
                srv.spawn(
                    rx.wrap().then(|_, _: &mut Master, srv: &mut CtxFramedService<MasterClient>| {
                        srv.send_buffered(MasterResponse::ServiceStopped);
                        fut::ok(())
                    })
                );
            }
        }
    }

    fn reload(&self, name: String, ctx: &mut Master,
              srv: &mut CtxFramedService<Self>, graceful: bool)
    {
        info!("Client command: Reload service '{}'", name);
        match ctx.cmd.borrow_mut().reload_service(name.as_str(), graceful) {
            Err(err) => self.handle_error(err, srv),
            Ok(rx) => {
                srv.spawn(
                    rx.wrap().then(|res, _: &mut _, srv: &mut CtxFramedService<MasterClient>| {
                        match res {
                            Ok(ReloadStatus::Success) =>
                                srv.send_buffered(MasterResponse::ServiceStarted),
                            Ok(ReloadStatus::Failed) =>
                                srv.send_buffered(MasterResponse::ServiceFailed),
                            Ok(ReloadStatus::Stopping) =>
                                srv.send_buffered(MasterResponse::ErrorServiceStopping),
                            Err(_) =>
                                srv.send_buffered(MasterResponse::ServiceFailed),
                        }
                        fut::ok(())
                    })
                );
            }
        }
    }

    fn start_service(&self, name: String, ctx: &mut Master, srv: &mut CtxFramedService<Self>) {
        info!("Client command: Start service '{}'", name);

        match ctx.cmd.borrow_mut().start_service(name.as_str()) {
            Err(err) => self.handle_error(err, srv),
            Ok(rx) => {
                srv.spawn(
                    rx.wrap().then(|res, _: &mut _, srv: &mut CtxFramedService<MasterClient>| {
                        match res {
                            Ok(StartStatus::Success) =>
                                srv.send_buffered(MasterResponse::ServiceStarted),
                            Ok(StartStatus::Failed) =>
                                srv.send_buffered(MasterResponse::ServiceFailed),
                            Ok(StartStatus::Stopping) =>
                                srv.send_buffered(MasterResponse::ErrorServiceStopping),
                            Err(_) =>
                                srv.send_buffered(MasterResponse::ServiceFailed),
                        }
                        fut::ok(())
                    })
                );
            }
        }
    }
}

impl FramedContextAware for MasterClient {

    type Context = Master;
    type FramedContext = UnixStream;
    type Codec = MasterTransportCodec;
    type Message = Result<MasterClientMessage, io::Error>;
    type Result = Result<(), ()>;

    fn start(&mut self, ctx: &mut Master, srv: &mut CtxFramedService<Self>) {
        ctx.hb(srv);
    }

    fn finished(&mut self, _: &mut Master, _: &mut CtxFramedService<Self>)
                -> Result<Async<()>, ()>
    {
        debug!("Client connection finished");
        Ok(Async::Ready(()))
    }

    fn call(&mut self,
            ctx: &mut Master,
            srv: &mut CtxFramedService<Self>, msg: CtxFramedResult<Self>)
            -> Result<Async<()>, ()>
    {
        match msg {
            CtxFramedResult::Ok(MasterClientMessage::Request(req)) => {
                match req {
                    MasterRequest::Ping =>
                        srv.send_buffered(MasterResponse::Pong),
                    MasterRequest::Start(name) =>
                        self.start_service(name, ctx, srv),
                    MasterRequest::Reload(name) =>
                        self.reload(name, ctx, srv, true),
                    MasterRequest::Restart(name) =>
                        self.reload(name, ctx, srv, false),
                    MasterRequest::Stop(name) =>
                        self.stop(name, ctx, srv),
                    MasterRequest::Pause(name) => {
                        info!("Client command: Pause service '{}'", name);
                        match ctx.cmd.borrow_mut().pause_service(name.as_str()) {
                            Err(err) => self.handle_error(err, srv),
                            Ok(_) => srv.send_buffered(MasterResponse::Done)
                        }
                    }
                    MasterRequest::Resume(name) => {
                        info!("Client command: Resume service '{}'", name);
                        match ctx.cmd.borrow_mut().resume_service(name.as_str()) {
                            Err(err) => self.handle_error(err, srv),
                            Ok(_) => srv.send_buffered(MasterResponse::Done)
                        }
                    }
                    MasterRequest::Status(name) => {
                        info!("Client command: Service status '{}'", name);
                        match ctx.cmd.borrow().service_status(name.as_str()) {
                            Ok(s) => srv.send_buffered(
                                MasterResponse::ServiceStatus(s.to_owned())),
                            Err(err) => self.handle_error(err, srv),
                        }
                    }
                    MasterRequest::Pid => {
                        srv.send_buffered(MasterResponse::Pid(
                            format!("{}", nix::unistd::getpid())));
                    },
                    MasterRequest::Version => {
                        srv.send_buffered(MasterResponse::Version(
                            format!("{} {}", PKG_INFO.name, PKG_INFO.version)));
                    },
                    MasterRequest::Quit => {
                        let rx = ctx.cmd.borrow_mut().stop();
                        srv.spawn(
                            rx.wrap().then(|_, _: &mut _, srv: &mut CtxFramedService<Self>| {
                                srv.send_buffered(MasterResponse::Done);
                                fut::ok(())
                            }));
                    }
                };
                Ok(Async::NotReady)
            },
            CtxFramedResult::Sent =>
                Ok(Async::NotReady),
            CtxFramedResult::Err(_) | CtxFramedResult::SinkErr(_) =>
                Err(()),
        }
    }
}

struct MasterListener;

impl ContextAware for MasterListener {
    type Context = Master;
    type Message = Result<(UnixStream, std::os::unix::net::SocketAddr), io::Error>;
    type Result = Result<(), ()>;

    fn finished(&mut self, _: &mut Master, _: &mut CtxService<Self>) -> Result<Async<()>, ()> {
        Ok(Async::Ready(()))
    }

    fn call(&mut self, _: &mut Master, srv: &mut CtxService<Self>, msg: Self::Message)
            -> Result<Async<()>, ()>
    {
        match msg {
            Ok((stream, _)) => {
                MasterClient.spawn(srv, stream.framed(MasterTransportCodec));
            }
            _ => (),
        }
        Ok(Async::NotReady)
    }
}

/// Codec for Master transport
struct MasterTransportCodec;

impl Decoder for MasterTransportCodec
{
    type Item = MasterClientMessage;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let size = {
            if src.len() < 2 {
                return Ok(None)
            }
            BigEndian::read_u16(src.as_ref()) as usize
        };

        if src.len() >= size + 2 {
            src.split_to(2);
            let buf = src.split_to(size);
            Ok(Some(MasterClientMessage::Request(json::from_slice::<MasterRequest>(&buf)?)))
        } else {
            Ok(None)
        }
    }
}

impl Encoder for MasterTransportCodec
{
    type Item = MasterResponse;
    type Error = io::Error;

    fn encode(&mut self, msg: MasterResponse, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = json::to_string(&msg).unwrap();
        let msg_ref: &[u8] = msg.as_ref();

        dst.reserve(msg_ref.len() + 2);
        dst.put_u16::<BigEndian>(msg_ref.len() as u16);
        dst.put(msg_ref);

        Ok(())
    }
}

/// Start master process
pub fn start(cfg: Config) -> bool {
    // init logging
    logging::init_logging(&cfg.logging);

    info!("Starting fectl process");

    // change working dir
    if let Err(err) = nix::unistd::chdir::<OsStr>(cfg.master.directory.as_ref()) {
        error!("Can not change directory {:?} err: {}", cfg.master.directory, err);
        return false
    }

    // create commands listener and also check if service process is running
    let lst = match StdUnixListener::bind(&cfg.master.sock) {
        Ok(lst) => lst,
        Err(err) => match err.kind() {
            io::ErrorKind::PermissionDenied => {
                error!("Can not create socket file {:?} err: Permission denied.",
                       cfg.master.sock);
                return false
            },
            io::ErrorKind::AddrInUse => {
                match client::is_alive(&cfg.master) {
                    client::AliveStatus::Alive => {
                        error!("Can not start: Service is running.");
                        return false
                    },
                    client::AliveStatus::NotResponding => {
                        error!("Master process is not responding.");
                        if let Some(pid) = cfg.master.load_pid() {
                            error!("Master process: (pid:{})", pid);
                        } else {
                            error!("Can not load pid of the master process.");
                        }
                        return false
                    },
                    client::AliveStatus::NotAlive => {
                        // remove socket and try again
                        let _ = std::fs::remove_file(&cfg.master.sock);
                        match StdUnixListener::bind(&cfg.master.sock) {
                            Ok(lst) => lst,
                            Err(err) => {
                                error!("Can not create listener socket: {:?}", err);
                                return false
                            }
                        }
                    }
                }
            }
            _ => {
                error!("Can not create listener socket: {}", err);
                return false
            }
        }
    };

    // try to save pid
    if let Err(err) = cfg.master.save_pid() {
        error!("Can not write pid file {:?} err: {}", cfg.master.pid, err);
        return false
    }

    // set uid
    if let Some(uid) = cfg.master.uid {
        if let Err(err) = nix::unistd::setuid(uid) {
            error!("Can not set process uid, err: {}", err);
            return false
        }
    }

    // set gid
    if let Some(gid) = cfg.master.gid {
        if let Err(err) = nix::unistd::setgid(gid) {
            error!("Can not set process gid, err: {}", err);
            return false
        }
    }

    let daemon = cfg.master.daemon;
    if daemon {
        if let Err(err) = nix::unistd::daemon(true, false) {
            error!("Can not daemonize process: {}", err);
            return false
        }

        // close stdin
        let _ = nix::unistd::close(libc::STDIN_FILENO);

        // redirect stdout and stderr
        if let Some(ref stdout) = cfg.master.stdout {
            match std::fs::OpenOptions::new().append(true).create(true).open(stdout)
            {
                Ok(f) => {
                    let _ = nix::unistd::dup2(f.as_raw_fd(), libc::STDOUT_FILENO);
                }
                Err(err) =>
                    error!("Can open stdout file {}: {}", stdout, err),
            }
        }
        if let Some(ref stderr) = cfg.master.stderr {
            match std::fs::OpenOptions::new().append(true).create(true).open(stderr)
            {
                Ok(f) => {
                    let _ = nix::unistd::dup2(f.as_raw_fd(), libc::STDERR_FILENO);

                },
                Err(err) => error!("Can open stderr file {}: {}", stderr, err)
            }
        }

        // continue start process
        nix::sys::stat::umask(nix::sys::stat::Mode::from_bits(0o22).unwrap());
    }

    // start
    let result = Master::start(cfg, lst);

    if !daemon {
        println!("");
    }

    result
}
