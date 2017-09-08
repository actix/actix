use std;
use std::io;
use std::error::Error;
use std::os::unix::io::RawFd;
use std::time::{Duration, Instant};

use serde_json as json;
use futures::{unsync, future, Async, Future, Stream};
use byteorder::{ByteOrder, BigEndian};
use bytes::{BytesMut, BufMut};
use tokio_core::reactor;
use tokio_core::reactor::Timeout;
use tokio_io::AsyncRead;
use tokio_io::codec::{Encoder, Decoder};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{close, pipe, fork, ForkResult, Pid};

use ctx::{Service, FramedContextAware, CtxFramedService, CtxFramedResult};

use config::ServiceConfig;
use io::PipeFile;
use worker::{WorkerMessage, WorkerCommand};
use event::Reason;
use exec::exec_worker;

const HEARTBEAT: u64 = 2;
const WORKER_TIMEOUT: i8 = 98;
pub const WORKER_INIT_FAILED: i8 = 99;
pub const WORKER_BOOT_FAILED: i8 = 100;


#[derive(PartialEq, Debug)]
pub enum ProcessCommand {
    Message(WorkerCommand),
    Start,
    Pause,
    Resume,
    Stop,
    Quit,
}

#[derive(Debug)]
pub enum ProcessNotification {
    /// Worker process message
    Message(Pid, WorkerMessage),

    /// Process heartbeat failed
    Failed(Pid, ProcessError),

    /// Process heartbeat failed
    Loaded(Pid),
}

pub struct Process {
    pid: Pid,
    state: ProcessState,
    hb: Instant,
    cmd: unsync::mpsc::UnboundedSender<ProcessNotification>,
    timeout: Duration,
    shutdown_timeout: u64,
}

#[derive(Debug)]
enum ProcessState {
    Starting,
    Failed,
    Running,
    Stopping,
}

#[derive(PartialEq, Debug)]
enum ProcessMessage {
    Message(WorkerMessage),
    Command(ProcessCommand),
    StartupTimeout,
    StopTimeout,
    Heartbeat,
    Kill,
}

#[derive(Debug)]
pub enum ProcessError {
    /// Heartbeat failed
    Heartbeat,
    /// Worker startup process failed, possibly application initialization failed
    FailedToStart(Option<io::Error>),
    /// Timeout during startup
    StartupTimeout,
    /// Timeout during graceful stop
    StopTimeout,
    /// Worker configuratin error
    ConfigError(String),
    /// Worker init failed
    InitFailed,
    /// Worker boot failed
    BootFailed,
    /// Worker received signal
    Signal(usize),
    /// Worker exited with code
    ExitCode(i8),
}

impl ProcessError {
    pub fn from(code: i8) -> ProcessError {
        match code {
            WORKER_TIMEOUT => ProcessError::StartupTimeout,
            WORKER_INIT_FAILED => ProcessError::InitFailed,
            WORKER_BOOT_FAILED => ProcessError::BootFailed,
            code => ProcessError::ExitCode(code),
        }
    }
}

impl<'a> std::convert::From<&'a ProcessError> for Reason
{
    fn from(ob: &'a ProcessError) -> Self {
        match ob {
            &ProcessError::Heartbeat => Reason::HeartbeatFailed,
            &ProcessError::FailedToStart(ref err) =>
                Reason::FailedToStart(
                    if let &Some(ref e) = err { Some(format!("{}", e))} else {None}),
            &ProcessError::StartupTimeout => Reason::StartupTimeout,
            &ProcessError::StopTimeout => Reason::StopTimeout,
            &ProcessError::ConfigError(ref err) => Reason::WorkerError(err.clone()),
            &ProcessError::InitFailed => Reason::InitFailed,
            &ProcessError::BootFailed => Reason::BootFailed,
            &ProcessError::Signal(sig) => Reason::Signal(sig),
            &ProcessError::ExitCode(code) => Reason::ExitCode(code),
        }
    }
}


impl Process {

    pub fn start(handle: &reactor::Handle, cfg: &ServiceConfig,
                 cmd: unsync::mpsc::UnboundedSender<ProcessNotification>)
                 -> (Pid, unsync::mpsc::UnboundedSender<ProcessCommand>)
    {
        let (tx, rx) = unsync::mpsc::unbounded();

        // fork process and esteblish communication
        let (pid, pipe) = match Process::fork(handle, cfg) {
            Ok(res) => res,
            Err(err) => {
                let pid = Pid::from_raw(-1);
                let _ = cmd.send(
                    ProcessNotification::Failed(pid, ProcessError::FailedToStart(Some(err))));

                return (pid, tx)
            }
        };

        // initiate loading procesdure
        let process = Process {
            pid: pid,
            state: ProcessState::Starting,
            hb: Instant::now(),
            cmd: cmd,
            timeout: Duration::new(cfg.timeout as u64, 0),
            shutdown_timeout: cfg.shutdown_timeout as u64,
        };
        ProcessManagement.build(process, pipe.framed(TransportCodec), handle)
            .add_future(
                Timeout::new(Duration::new(cfg.startup_timeout as u64, 0), &handle).unwrap()
                    .map(|_| ProcessMessage::StartupTimeout)
            )
            .add_stream(
                rx.then(|res| match res {
                    Ok(cmd) => future::ok(ProcessMessage::Command(cmd)),
                    Err(_) => future::ok(ProcessMessage::Command(ProcessCommand::Quit)),
                })
            )
            .run();

        (pid, tx)
    }

    fn fork(handle: &reactor::Handle, cfg: &ServiceConfig) -> Result<(Pid, PipeFile), io::Error>
    {
        let (p_read, p_write, ch_read, ch_write) = Process::create_pipes()?;

        // fork
        let pid = match fork() {
            Ok(ForkResult::Parent{ child }) => child,
            Ok(ForkResult::Child) => {
                let _ = close(p_write);
                let _ = close(ch_read);
                exec_worker(cfg, p_read, ch_write);
                unreachable!();
            },
            Err(err) => {
                error!("Fork failed: {}", err.description());
                return Err(io::Error::new(io::ErrorKind::Other, err.description()))
            }
        };

        // initialize worker communication channel
        let _ = close(p_read);
        let _ = close(ch_write);
        let pipe = PipeFile::new(ch_read, p_write, handle);

        Ok((pid, pipe))
    }

    fn create_pipes() -> Result<(RawFd, RawFd, RawFd, RawFd), io::Error> {
        // open communication pipes
        let (p_read, p_write) = match pipe() {
            Ok((r, w)) => (r, w),
            Err(err) => {
                error!("Can not create pipe: {}", err);
                return Err(io::Error::new(
                    io::ErrorKind::Other, format!("Can not create pipe: {}", err)))
            }
        };
        let (ch_read, ch_write) = match pipe() {
            Ok((r, w)) => (r, w),
            Err(err) => {
                error!("Can not create pipe: {}", err);
                return Err(io::Error::new(
                    io::ErrorKind::Other, format!("Can not create pipe: {}", err)))
            }
        };
        Ok((p_read, p_write, ch_read, ch_write))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = kill(self.pid, Signal::SIGKILL);
    }
}

struct ProcessManagement;

impl ProcessManagement {

    fn kill(&self, srv: &mut CtxFramedService<Self>) {
        let fut = Box::new(
            Timeout::new(Duration::new(1, 0), srv.handle())
                .unwrap()
                .map(|_| ProcessMessage::Kill));
        srv.add_future(fut);
    }
}

impl FramedContextAware for ProcessManagement {

    type Context = Process;
    type FramedContext = PipeFile;
    type Codec = TransportCodec;
    type Message = Result<ProcessMessage, io::Error>;
    type Result = Result<(), ()>;

    fn finished(&mut self, _: &mut Process, srv: &mut CtxFramedService<Self>)
                -> Result<Async<()>, ()>
    {
        self.kill(srv);
        Ok(Async::NotReady)
    }

    fn call(&mut self,
            ctx: &mut Process,
            srv: &mut CtxFramedService<Self>, msg: CtxFramedResult<Self>)
            -> Result<Async<()>, ()>
    {
        match msg {
            Ok(ProcessMessage::Message(msg)) => match msg {
                WorkerMessage::forked => {
                    debug!("Worker forked (pid:{})", ctx.pid);
                    srv.send_buffered(WorkerCommand::prepare);
                }
                WorkerMessage::loaded => {
                    match ctx.state {
                        ProcessState::Starting => {
                            debug!("Worker loaded (pid:{})", ctx.pid);

                            ctx.state = ProcessState::Running;

                            if let Err(_) = ctx.cmd.send(ProcessNotification::Loaded(ctx.pid)) {
                                // parent is dead
                                return self.call(
                                    ctx, srv, Ok(ProcessMessage::Command(ProcessCommand::Quit)))
                            } else {
                                // start heartbeat timer
                                ctx.hb = Instant::now();
                                let fut = Box::new(
                                    Timeout::new(
                                        Duration::new(HEARTBEAT, 0), srv.handle())
                                        .unwrap()
                                        .map(|_| ProcessMessage::Heartbeat));
                                srv.add_future(fut);
                            }
                        },
                        _ => {
                            warn!("Received `loaded` message from worker (pid:{})", ctx.pid);
                        }
                    }
                }
                WorkerMessage::hb => {
                    ctx.hb = Instant::now();
                }
                WorkerMessage::reload => {
                    // worker requests reload
                    info!("Worker requests reload (pid:{})", ctx.pid);
                    if let Err(_) = ctx.cmd.send(
                        ProcessNotification::Message(ctx.pid, WorkerMessage::reload)) {
                        // parent is dead
                        return self.call(
                            ctx, srv, Ok(ProcessMessage::Command(ProcessCommand::Quit)))
                    }
                }
                WorkerMessage::restart => {
                    // worker requests reload
                    info!("Worker requests restart (pid:{})", ctx.pid);
                    if let Err(_) = ctx.cmd.send(
                        ProcessNotification::Message(ctx.pid, WorkerMessage::restart)) {
                        // parent is dead
                        return self.call(
                            ctx, srv, Ok(ProcessMessage::Command(ProcessCommand::Quit)))
                    }
                }
                WorkerMessage::cfgerror(msg) => {
                    error!("Worker config error: {} (pid:{})", msg, ctx.pid);
                    if let Err(_) = ctx.cmd.send(ProcessNotification::Failed(
                        ctx.pid, ProcessError::ConfigError(msg)))
                    {
                        // parent is dead
                        return self.call(
                            ctx, srv, Ok(ProcessMessage::Command(ProcessCommand::Quit)))
                    }
                }
            }
            Ok(ProcessMessage::StartupTimeout) => {
                match ctx.state {
                    ProcessState::Starting => {
                        debug!("Worker startup timeout");
                        ctx.state = ProcessState::Failed;
                        let _ = ctx.cmd.send(ProcessNotification::Failed(
                            ctx.pid, ProcessError::StartupTimeout));
                        let _ = kill(ctx.pid, Signal::SIGKILL);
                        return Ok(Async::Ready(()))
                    },
                    _ => ()
                }
            }
            Ok(ProcessMessage::StopTimeout) => {
                match ctx.state {
                    ProcessState::Stopping => {
                        debug!("Worker shutdown timeout");
                        ctx.state = ProcessState::Failed;
                        let _ = ctx.cmd.send(ProcessNotification::Failed(
                            ctx.pid, ProcessError::StopTimeout));
                        let _ = kill(ctx.pid, Signal::SIGKILL);
                        return Ok(Async::Ready(()))
                    },
                    _ => ()
                }
            }
            Ok(ProcessMessage::Heartbeat) => {
                // makes sense only in running state
                if let ProcessState::Running = ctx.state {
                    if Instant::now().duration_since(ctx.hb) > ctx.timeout {
                        // heartbeat timed out
                        info!("Worker heartbeat failed (pid:{})", ctx.pid);
                        let _ = (&ctx.cmd).send(
                            ProcessNotification::Failed(ctx.pid, ProcessError::Heartbeat));
                    } else {
                        // send heartbeat to worker process and reset hearbeat timer
                        srv.send_buffered(WorkerCommand::hb);
                        let fut = Box::new(
                                Timeout::new(Duration::new(HEARTBEAT, 0), srv.handle())
                                    .unwrap()
                                    .map(|_| ProcessMessage::Heartbeat));
                        srv.add_future(fut);
                    }
                }
            }
            Ok(ProcessMessage::Kill) => {
                let _ = kill(ctx.pid, Signal::SIGKILL);
                return Ok(Async::Ready(()))
            }
            Ok(ProcessMessage::Command(cmd)) => match cmd {
                ProcessCommand::Message(cmd) =>
                    srv.send_buffered(cmd),
                ProcessCommand::Start =>
                    srv.send_buffered(WorkerCommand::start),
                ProcessCommand::Pause =>
                    srv.send_buffered(WorkerCommand::pause),
                ProcessCommand::Resume =>
                    srv.send_buffered(WorkerCommand::resume),
                ProcessCommand::Stop => {
                    info!("Stopping worker: (pid:{})", ctx.pid);
                    match ctx.state {
                        ProcessState::Running => {
                            srv.send_buffered(WorkerCommand::stop);

                            ctx.state = ProcessState::Stopping;
                            if let Ok(timeout) = Timeout::new(
                                Duration::new(ctx.shutdown_timeout, 0), srv.handle())
                            {
                                srv.add_future(
                                    timeout.map(|_| ProcessMessage::StopTimeout));
                                let _ = kill(ctx.pid, Signal::SIGTERM);
                            } else {
                                // can not create timeout
                                let _ = kill(ctx.pid, Signal::SIGQUIT);
                                return Ok(Async::Ready(()))
                            }
                        },
                        _ => {
                            let _ = kill(ctx.pid, Signal::SIGQUIT);
                            return Ok(Async::Ready(()))
                        }
                    }
                }
                ProcessCommand::Quit => {
                    let _ = kill(ctx.pid, Signal::SIGQUIT);
                    self.kill(srv)
                }
            }
            Err(_) => self.kill(srv),
        }
        Ok(Async::NotReady)
    }
}

struct TransportCodec;

impl Decoder for TransportCodec {
    type Item = ProcessMessage;
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
            Ok(Some(ProcessMessage::Message(json::from_slice::<WorkerMessage>(&buf)?)))
        } else {
            Ok(None)
        }
    }
}

impl Encoder for TransportCodec {
    type Item = WorkerCommand;
    type Error = io::Error;

    fn encode(&mut self, msg: WorkerCommand, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = json::to_string(&msg).unwrap();
        let msg_ref: &[u8] = msg.as_ref();

        dst.reserve(msg_ref.len() + 2);
        dst.put_u16::<BigEndian>(msg_ref.len() as u16);
        dst.put(msg_ref);

        Ok(())
    }
}
