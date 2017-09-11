#![allow(dead_code)]

use std;
use std::rc::Rc;
use std::cell::RefCell;

use futures::unsync::oneshot;
use futures::{unsync, Async, Stream};
use tokio_core::reactor;
use nix::unistd::Pid;

use ctx::{self, ContextAware, CtxService};
use event::{Event, Reason, ServiceStatus};
use config::ServiceConfig;
use worker::{Worker, WorkerMessage};
use process::{ProcessNotification, ProcessError};


#[derive(PartialEq, Debug)]
/// Service interface
pub enum ServiceCommand {
    // /// Gracefully reload workers
    // Reload,
    // /// Reconfigure active workers
    // Configure(usize, String),
    // /// Gracefully stopping workers
    // Stop,
    /// Quit all workers
    Quit,
}

/// Service state
#[derive(Debug)]
enum ServiceState {
    Running,
    Failed,
    Stopped,
    Starting(Task<StartStatus>),
    Reloading(Task<ReloadStatus>),
    Stopping(Task<()>),
}

impl ServiceState {
    fn description(&self) -> &'static str {
        match *self {
            ServiceState::Running => "running",
            ServiceState::Failed => "failed",
            ServiceState::Stopped => "stopped",
            ServiceState::Starting(_) => "starting",
            ServiceState::Reloading(_) => "reloading",
            ServiceState::Stopping(_) => "stopping",
        }
    }

    fn error(&self) -> ServiceOperationError {
        match *self {
            ServiceState::Running => ServiceOperationError::Running,
            ServiceState::Failed => ServiceOperationError::Failed,
            ServiceState::Stopped => ServiceOperationError::Stopped,
            ServiceState::Starting(_) => ServiceOperationError::Starting,
            ServiceState::Reloading(_) => ServiceOperationError::Reloading,
            ServiceState::Stopping(_) => ServiceOperationError::Stopping,
        }
    }
}

#[derive(Debug)]
/// Service errors
pub enum ServiceOperationError {
    Starting,
    Reloading,
    Stopping,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug)]
pub enum ServiceMessage {
    /// external command
    Command(ServiceCommand),
    /// process notification,
    Process(usize, ProcessNotification),
}

#[derive(Clone, Debug)]
pub enum StartStatus {
    Success,
    Failed,
    Stopping,
}

#[derive(Clone, Debug)]
pub enum ReloadStatus {
    Success,
    Failed,
    Stopping,
}

pub struct Service {
    name: String,
    state: ServiceState,
    paused: bool,
    workers: Vec<Worker>,
    tx: unsync::mpsc::UnboundedSender<ServiceCommand>,
}

impl Service {

    pub fn start(handle: &reactor::Handle, num: u16, cfg: ServiceConfig) -> Rc<RefCell<Service>>
    {
        let (tx, rx) = unsync::mpsc::unbounded();

        // create workers
        let mut workers = Vec::new();
        let mut notifications = Vec::new();
        for idx in 0..num as usize {
            let (tx, rx) = unsync::mpsc::unbounded();
            notifications.push(
                rx.map(move |msg| ServiceMessage::Process(idx, msg)));
            workers.push(Worker::new(0, handle, cfg.clone(), tx));
        }

        let mut srv = ServiceCommands.build(
            Service{
                name: cfg.name.clone(),
                state: ServiceState::Starting(Task::new()),
                paused: false,
                workers: workers,
                tx: tx}, rx.map(|cmd| ServiceMessage::Command(cmd)), handle);
        for rx in notifications {
            srv = srv.add_stream(rx);
        }

        srv.clone_and_run()
    }

    pub fn status(&self) -> ServiceStatus {
        let mut events: Vec<(String, Vec<Event>)> = Vec::new();
        for worker in self.workers.iter() {
            events.push(
                (format!("worker({})", worker.idx + 1), Vec::from(&worker.events)));
        }

        let status = match self.state {
            ServiceState::Running =>
                if self.paused { "paused" } else { "running" }
            _ => self.state.description()
        };
        (status.to_owned(), events)
    }

    pub fn pids(&self) -> Vec<String> {
        let mut pids = Vec::new();
        for worker in self.workers.iter() {
            if let Some(pid) = worker.pid() {
                pids.push(format!("{}", pid));
            }
        }
        pids
    }

    pub fn is_stopped(&self) -> bool {
        match self.state {
            ServiceState::Failed | ServiceState::Stopped => true,
            _ => false,
        }
    }

    fn check_loading_workers(&mut self, restart_stopped: bool) -> (bool, bool) {
        let mut in_process = false;
        let mut failed = false;

        for worker in self.workers.iter_mut() {
            if worker.is_failed() {
                failed = true;
            }
            else if worker.is_stopped() {
                if restart_stopped {
                    // strange
                    worker.reload(true, Reason::None);
                    in_process = true;
                }
            }
            else if !worker.is_running() {
                in_process = true;
            }
        }
        (failed, in_process)
    }

    // update internal state
    fn update(&mut self) {
        let state = std::mem::replace(&mut self.state, ServiceState::Failed);

        match state {
            ServiceState::Starting(task) => {
                let (failed, in_process) = self.check_loading_workers(true);

                // if we have failed workers, stop all and change service state to failed
                if failed {
                    if in_process {
                        for worker in self.workers.iter_mut() {
                            if !(worker.is_stopped() || worker.is_failed()) {
                                worker.stop(Reason::SomeWorkersFailed)
                            }
                        }
                        self.state = ServiceState::Starting(task);
                    } else {
                        task.set_result(StartStatus::Failed);
                        self.state = ServiceState::Failed;
                    }
                } else {
                    if !in_process {
                        task.set_result(StartStatus::Success);
                        self.state = ServiceState::Running;
                    } else {
                        self.state = ServiceState::Starting(task);
                    }
                }
            },
            ServiceState::Reloading(task) => {
                let (failed, in_process) = self.check_loading_workers(true);

                // if we have failed workers, stop all and change service state to failed
                if failed {
                    if in_process {
                        for worker in self.workers.iter_mut() {
                            if !(worker.is_stopped() || worker.is_failed()) {
                                worker.stop(Reason::SomeWorkersFailed)
                            }
                        }
                        self.state = ServiceState::Reloading(task);
                    } else {
                        task.set_result(ReloadStatus::Failed);
                        self.state = ServiceState::Failed;
                    }
                } else {
                    if !in_process {
                        task.set_result(ReloadStatus::Success);
                        self.state = ServiceState::Running;
                    } else {
                        self.state = ServiceState::Reloading(task);
                    }
                }
            },
            ServiceState::Stopping(task) => {
                let (_, in_process) = self.check_loading_workers(false);

                if !in_process {
                    task.set_result(());
                    self.state = ServiceState::Stopped;
                } else {
                    self.state = ServiceState::Stopping(task);
                }
            },
            state => self.state = state,
        }
    }
    
    pub fn exited(&mut self, pid: Pid, err: &ProcessError) {
        for worker in self.workers.iter_mut() {
            worker.exited(pid, err);
        }
        self.update();
    }

    fn message(&mut self, pid: Pid, message: WorkerMessage) {
        for worker in self.workers.iter_mut() {
            worker.message(pid, &message)
        }
    }

    pub fn start_service(&mut self)
                         -> Result<oneshot::Receiver<StartStatus>, ServiceOperationError>
    {
        match self.state {
            ServiceState::Starting(ref mut task) => {
                Ok(task.wait())
            }
            ServiceState::Failed | ServiceState::Stopped => {
                debug!("Starting service: {:?}", self.name);
                let mut task = Task::new();
                let rx = task.wait();
                self.paused = false;
                self.state = ServiceState::Starting(task);
                for worker in self.workers.iter_mut() {
                    worker.start(Reason::ConsoleRequest);
                }
                Ok(rx)
            }
            _ => Err(self.state.error())
        }
    }

    pub fn pause(&mut self) -> Result<(), ServiceOperationError>
    {
        match self.state {
            ServiceState::Running => {
                debug!("Pause service: {:?}", self.name);
                for worker in self.workers.iter_mut() {
                    worker.pause(Reason::ConsoleRequest);
                }
                self.paused = true;
                return Ok(())
            }
            _ => Err(self.state.error())
        }
    }

    pub fn resume(&mut self) -> Result<(), ServiceOperationError>
    {
        match self.state {
            ServiceState::Running => {
                debug!("Resume service: {:?}", self.name);
                for worker in self.workers.iter_mut() {
                    worker.resume(Reason::ConsoleRequest);
                }
                self.paused = false;
                return Ok(())
            }
            _ => Err(self.state.error())
        }
    }

    pub fn reload(&mut self, graceful: bool)
                 -> Result<oneshot::Receiver<ReloadStatus>, ServiceOperationError>
    {
        match self.state {
            ServiceState::Reloading(ref mut task) => {
                Ok(task.wait())
            }
            ServiceState::Running | ServiceState::Failed | ServiceState::Stopped => {
                debug!("Reloading service: {:?}", self.name);
                let mut task = Task::new();
                let rx = task.wait();
                self.paused = false;
                self.state = ServiceState::Reloading(task);
                for worker in self.workers.iter_mut() {
                    worker.reload(graceful, Reason::ConsoleRequest);
                }
                return Ok(rx)
            }
            _ => Err(self.state.error())
        }
    }

    pub fn stop(&mut self, graceful: bool, reason: Reason) -> Result<oneshot::Receiver<()>, ()>
    {
        let state = std::mem::replace(&mut self.state, ServiceState::Stopped);

        match state {
            ServiceState::Failed | ServiceState::Stopped => {
                self.state = state;
                return Err(())
            },
            ServiceState::Stopping(mut task) => {
                let rx = task.wait();
                self.state = ServiceState::Stopping(task);
                return Ok(rx)
            },
            ServiceState::Starting(task) => {
                task.set_result(StartStatus::Stopping);
            }
            ServiceState::Reloading(task) => {
                task.set_result(ReloadStatus::Stopping);
            }
            ServiceState::Running => ()
        }

        // stop workers
        let mut task = Task::new();
        let rx = task.wait();
        self.paused = false;
        self.state = ServiceState::Stopping(task);
        for worker in self.workers.iter_mut() {
            if graceful {
                worker.stop(reason.clone());
            } else {
                worker.quit(reason.clone());
            }
        }
        self.update();
        Ok(rx)
    }
}


pub struct ServiceCommands;

impl ctx::ContextAware for ServiceCommands {
    type Context = Service;
    type Message = Result<ServiceMessage, ()>;
    type Result = Result<(), ()>;

    fn start(&mut self, ctx: &mut Service, _: &mut CtxService<Self>) {
        for worker in ctx.workers.iter_mut() {
            worker.start(Reason::Initial);
        }
    }

    fn finished(&mut self, _: &mut Service, _: &mut CtxService<Self>) -> Result<Async<()>, ()> {
        // command center probably dead
        Ok(Async::Ready(()))
    }

    fn call(&mut self,
            ctx: &mut Service,
            _srv: &mut CtxService<Self>,
            cmd: Result<ServiceMessage, ()>) -> Result<Async<()>, ()>
    {
        match cmd {
            // Ok(ServiceMessage::Command(ServiceCommand::Reload)) => {
            //    let _ = ctx.reload(true);
            //}
            // CtxResult::Ok(ServiceCommand::Configure(_num, _exec)) => {
            // }
            // Ok(ServiceMessage::Command(ServiceCommand::Stop)) => {
            //    let _ = ctx.stop(true);
            // }
            Ok(ServiceMessage::Command(ServiceCommand::Quit)) => {
                let _ = ctx.stop(false, Reason::Exit);
            }

            Ok(ServiceMessage::Process(id, ProcessNotification::Message(pid, msg))) =>
            {
                ctx.workers[id].message(pid, &msg);
                ctx.update();
            }
            Ok(ServiceMessage::Process(id, ProcessNotification::Failed(pid, err))) =>
            {
                ctx.workers[id].exited(pid, &err);
                ctx.update();
            },
            Ok(ServiceMessage::Process(id, ProcessNotification::Loaded(pid))) =>
            {
                ctx.workers[id].loaded(pid);
                ctx.update();
            }
            Err(_) =>
                return Ok(Async::Ready(())), // command center probably dead
        }

        Ok(Async::NotReady)
    }
}


#[derive(Debug)]
struct Task<T> where T: Clone + std::fmt::Debug {
    waiters: Vec<unsync::oneshot::Sender<T>>,
}

impl<T> Task<T> where T: Clone + std::fmt::Debug {

    fn new() -> Task<T> {
        Task { waiters: Vec::new() }
    }

    fn wait(&mut self) -> unsync::oneshot::Receiver<T> {
        let (tx, rx) = unsync::oneshot::channel();
        self.waiters.push(tx);
        rx
    }

    fn set_result(self, result: T) {
        for waiter in self.waiters {
            let _ = waiter.send(result.clone());
        }
    }
}
