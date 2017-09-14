use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

use libc;
use futures::unsync::oneshot;
use futures::{unsync, Async, Future, Stream};
use tokio_core::reactor;
use tokio_signal;
use tokio_signal::unix::Signal;
use nix::unistd::getpid;
use nix::sys::wait::{waitpid, WaitStatus, WNOHANG};

use ctx::fut::{self, CtxFuture, WrapFuture};
use ctx::{Service as _Service, CtxService, CtxServiceStream, ContextAware};
use config::Config;
use event::{Reason, ServiceStatus};
use process::ProcessError;
use service::{Service, StartStatus, ReloadStatus, ServiceOperationError};

#[derive(Debug)]
/// Command center errors
pub enum CommandError {
    /// command center is not in Running state
    NotReady,
    /// service is not known
    UnknownService,
    /// service is stopped
    ServiceStopped,
    /// underlying service error
    Service(ServiceOperationError),
}

#[derive(PartialEq, Debug)]
enum State {
    Starting,
    Running,
    Stopping,
}

#[derive(Debug)]
enum Command {
    Stop,
    Quit,
    Reload,
    ReapWorkers,
}

pub struct CommandCenter {
    cfg: Rc<Config>,
    state: State,
    handle: reactor::Handle,
    stop: Option<unsync::oneshot::Sender<bool>>,
    tx: unsync::mpsc::UnboundedSender<Command>,
    services: HashMap<String, Rc<RefCell<Service>>>,
    stop_waiters: Vec<unsync::oneshot::Sender<bool>>,
}

impl CommandCenter {

    pub fn new(cfg: Rc<Config>, handle: &reactor::Handle, stop: unsync::oneshot::Sender<bool>)
               -> Rc<RefCell<CommandCenter>> {
        let (cmd_tx, cmd_rx) = unsync::mpsc::unbounded();

        let cmd = CommandCenter {
            cfg: cfg,
            state: State::Starting,
            handle: handle.clone(),
            stop: Some(stop),
            tx: cmd_tx,
            services: HashMap::new(),
            stop_waiters: Vec::new(),
        };

        // start command center
        CommandCenterCommands.clone_and_run(cmd, cmd_rx, &handle)
    }

    fn exit(&mut self, success: bool) {
        while let Some(waiter) = self.stop_waiters.pop() {
            let _ = waiter.send(true);
        }

        if let Some(stop) = self.stop.take() {
            let _ = stop.send(success);
        }
    }

    pub fn stop(&mut self) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        self.stop_waiters.push(tx);
        let _ = self.tx.unbounded_send(Command::Stop);
        rx
    }

    pub fn service_status(&self, name: &str) -> Result<ServiceStatus, CommandError>
    {
        match self.state {
            State::Running => {
                match self.services.get(name) {
                    Some(service) => Ok(service.borrow().status()),
                    None => Err(CommandError::UnknownService),
                }
            }
            _ => {
                Err(CommandError::NotReady)
            }
        }
    }

    pub fn service_worker_pids(&self, name: &str) -> Result<Vec<String>, CommandError>
    {
        match self.state {
            State::Running => {
                match self.services.get(name) {
                    Some(service) => Ok(service.borrow().pids()),
                    None => Err(CommandError::UnknownService),
                }
            }
            _ => {
                Err(CommandError::NotReady)
            }
        }
    }

    /// Start Service by `name`
    pub fn start_service(&mut self, name: &str)
                         -> Result<oneshot::Receiver<StartStatus>, CommandError>
    {
        match self.state {
            State::Running => {
                info!("Starting service {:?}", name);
                match self.services.get_mut(name) {
                    Some(service) => match service.borrow_mut().start_service() {
                        Ok(rx) => Ok(rx),
                        Err(err) => Err(CommandError::Service(err))
                    }
                    None => Err(CommandError::UnknownService)
                }
            }
            _ => {
                warn!("Can not reload in system in `{:?}` state", self.state);
                Err(CommandError::NotReady)
            }
        }
    }

    /// stop Service by name
    pub fn stop_service(&mut self, name: &str, graceful: bool)
                        -> Result<oneshot::Receiver<()>, CommandError>
    {
        match self.state {
            State::Running => {
                info!("Stopping service {:?}", name);
                match self.services.get_mut(name) {
                    Some(service) => match service.borrow_mut().stop(
                        graceful, Reason::ConsoleRequest)
                    {
                        Ok(rx) => Ok(rx),
                        Err(_) => Err(CommandError::ServiceStopped),
                    },
                    None => Err(CommandError::UnknownService),
                }
            }
            _ => {
                warn!("Can not reload in system in `{:?}` state", self.state);
                Err(CommandError::NotReady)
            }
        }
    }

    /// reload Service by `name`
    pub fn reload_service(&mut self, name: &str, graceful: bool)
                          -> Result<oneshot::Receiver<ReloadStatus>, CommandError>
    {
        match self.state {
            State::Running => {
                info!("Reloading service {:?}", name);
                match self.services.get_mut(name) {
                    Some(service) => match service.borrow_mut().reload(graceful) {
                        Ok(rx) => Ok(rx),
                        Err(err) => Err(CommandError::Service(err))
                    }
                    None => Err(CommandError::UnknownService)
                }
            }
            _ => {
                warn!("Can not reload in system in `{:?}` state", self.state);
                Err(CommandError::NotReady)
            }
        }
    }

    pub fn pause_service(&mut self, name: &str) -> Result<(), CommandError>
    {
        match self.state {
            State::Running => {
                info!("Pause service {:?}", name);
                match self.services.get_mut(name) {
                    Some(service) => match service.borrow_mut().pause() {
                        Ok(_) => Ok(()),
                        Err(err) => Err(CommandError::Service(err))
                    }
                    None => Err(CommandError::UnknownService)
                }
            }
            _ => {
                warn!("Can not reload in system in `{:?}` state", self.state);
                Err(CommandError::NotReady)
            }
        }
    }

    pub fn resume_service(&mut self, name: &str) -> Result<(), CommandError>
    {
        match self.state {
            State::Running => {
                info!("Resume service {:?}", name);
                match self.services.get_mut(name) {
                    Some(service) => match service.borrow_mut().resume() {
                        Ok(_) => Ok(()),
                        Err(err) => Err(CommandError::Service(err))
                    }
                    None => Err(CommandError::UnknownService)
                }
            }
            _ => {
                warn!("Can not reload in system in `{:?}` state", self.state);
                Err(CommandError::NotReady)
            }
        }
    }

    /// reload all services
    pub fn reload_all(&mut self) {
        match self.state {
            State::Running => {
                info!("reloading all services");
                for srv in self.services.values() {
                    let _ = srv.borrow_mut().reload(true);
                }
            }
            _ => warn!("Can not reload in system in `{:?}` state", self.state)
        }
    }
}

struct CommandCenterCommands;

impl CommandCenterCommands {

    fn init_signals(&self, srv: &mut CtxService<Self>) {
        let handle = srv.handle().clone();

        // SIGHUP
        srv.add_fut_stream(
            Box::new(
                Signal::new(libc::SIGHUP, &handle)
                    .map(|sig| Box::new(sig.map(|_| {
                        info!("SIGHUP received, reloading");
                        Command::Reload}).map_err(|_| ()))
                         as Box<CtxServiceStream<CommandCenterCommands>>)
                    .map_err(|_| ()))
        );

        // SIGTERM
        srv.add_fut_stream(
            Box::new(
                Signal::new(libc::SIGTERM, &handle)
                    .map(|sig| Box::new(sig.map(|_| {
                        info!("SIGTERM received, stopping");
                        Command::Stop}).map_err(|_| ()))
                         as Box<CtxServiceStream<CommandCenterCommands>>)
                    .map_err(|_| ()))
        );

        // SIGINT
        srv.add_fut_stream(
            Box::new(
                tokio_signal::ctrl_c(&handle)
                    .map(|sig| Box::new(sig.map(|_| {
                        info!("SIGINT received, exiting");
                        Command::Quit}).map_err(|_| ()))
                         as Box<CtxServiceStream<CommandCenterCommands>>)
                    .map_err(|_| ()))
        );

        // SIGQUIT
        srv.add_fut_stream(
            Box::new(
                Signal::new(libc::SIGQUIT, &handle)
                    .map(|sig| Box::new(sig.map(|_| {
                        info!("SIGQUIT received, exiting");
                        Command::Quit}).map_err(|_| ()))
                         as Box<CtxServiceStream<CommandCenterCommands>>)
                    .map_err(|_| ()))
        );

        // SIGCHLD
        srv.add_fut_stream(
            Box::new(
                Signal::new(libc::SIGCHLD, &handle)
                    .map(|sig| Box::new(sig.map(|_| {
                        debug!("SIGCHLD received");
                        Command::ReapWorkers}).map_err(|_| ()))
                         as Box<CtxServiceStream<CommandCenterCommands>>)
                    .map_err(|_| ()))
        );
    }
    
    fn stop(&self, ctx: &mut CommandCenter, srv: &mut CtxService<Self>, graceful: bool)
    {
        if ctx.state != State::Stopping {
            info!("Stopping service");

            ctx.state = State::Stopping;
            let mut waiting = false;
            for service in ctx.services.values() {
                match service.borrow_mut().stop(graceful, Reason::Exit) {
                    Ok(rx) => {
                        waiting = true;
                        srv.spawn(
                            rx.wrap().then(|_, ctx: &mut CommandCenter, _: &mut CtxService<CommandCenterCommands>| {
                                // check if all services are stopped
                                for srv in ctx.services.values() {
                                    if !srv.borrow().is_stopped() {
                                        return fut::ok(())
                                    }
                                }
                                ctx.exit(true);
                                return fut::ok(())
                            }));
                    }
                    Err(_) => (),
                }
            }
            if !waiting {
                ctx.exit(true);
            }
        }
    }
}

impl ContextAware for CommandCenterCommands {

    type State = CommandCenter;
    type Message = Result<Command, ()>;
    type Result = Result<(), ()>;

    fn start(&mut self, ctx: &mut CommandCenter, srv: &mut CtxService<Self>)
    {
        info!("Starting ctl service: {}", getpid());
        self.init_signals(srv);

        // start services
        for cfg in ctx.cfg.services.iter() {
            let service = Service::start(&ctx.handle, cfg.num, cfg.clone());
            ctx.services.insert(cfg.name.clone(), service);
        }
        ctx.state = State::Running;
    }

    fn finished(&mut self, ctx: &mut CommandCenter, _: &mut CtxService<Self>)
                -> Result<Async<()>, ()>
    {
        ctx.exit(true);
        Ok(Async::Ready(()))
    }

    fn call(&mut self, ctx: &mut CommandCenter, srv: &mut CtxService<Self>, cmd: Self::Message)
            -> Result<Async<()>, ()>
    {
        match cmd {
            Ok(Command::Stop) => {
                self.stop(ctx, srv, true);
            }
            Ok(Command::Quit) => {
                self.stop(ctx, srv, false);
            }
            Ok(Command::Reload) => {
                ctx.reload_all();
            }
            Ok(Command::ReapWorkers) => {
                debug!("Reap workers");
                loop {
                    match waitpid(None, Some(WNOHANG)) {
                        Ok(WaitStatus::Exited(pid, code)) => {
                            info!("Worker {} exit code: {}", pid, code);
                            let err = ProcessError::from(code);
                            for srv in ctx.services.values_mut() {
                                srv.borrow_mut().exited(pid, &err);
                            }
                            continue
                        }
                        Ok(WaitStatus::Signaled(pid, sig, _)) => {
                            info!("Worker {} exit by signal {:?}", pid, sig);
                            let err = ProcessError::Signal(sig as usize);
                            for srv in ctx.services.values_mut() {
                                srv.borrow_mut().exited(pid, &err);
                            }
                            continue
                        },
                        Ok(_) => (),
                        Err(_) => (),
                    }
                    break
                }
            }
            Err(_) => {
                ctx.exit(false);
                return Ok(Async::Ready(()))
            }
        }

        Ok(Async::NotReady)
    }
}
