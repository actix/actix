use std;
use std::time::{Duration, Instant};

use nix::unistd::Pid;
use futures::unsync;
use tokio_core::reactor;

use utils::str;
use event::{Events, State, Reason};
use config::ServiceConfig;
use process::{Process, ProcessCommand, ProcessNotification, ProcessError};

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag="cmd", content="data")]
pub enum WorkerCommand {
    prepare,
    start,
    pause,
    resume,
    stop,
    /// master heartbeat
    hb,
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag="cmd", content="data")]
pub enum WorkerMessage {
    /// ready to execute worker in forked process
    forked,
    /// worker loaded
    loaded,
    /// worker requests reload
    reload,
    /// worker requests restart
    restart,
    /// worker configuration error
    cfgerror(String),
    /// heartbeat
    hb,
}

#[derive(Debug)]
enum WorkerState {
    Initial,
    Starting(ProcessInfo),
    Reloading(ProcessInfo, ProcessInfo),
    Restarting(ProcessInfo, ProcessInfo),
    Running(ProcessInfo),
    StoppingOld(ProcessInfo, ProcessInfo),
    Stopping(ProcessInfo),
    Failed,
    Stopped,
}

#[derive(Debug)]
struct ProcessInfo {
    pid: Pid,
    tx: unsync::mpsc::UnboundedSender<ProcessCommand>,
}

impl ProcessInfo {
    fn stop(&self) {
        let _ = self.tx.send(ProcessCommand::Stop);
    }
    fn quit(&self) {
        let _ = self.tx.send(ProcessCommand::Quit);
    }
    fn start(&self) {
        let _ = self.tx.send(ProcessCommand::Start);
    }
    fn pause(&self) {
        let _ = self.tx.send(ProcessCommand::Pause);
    }
    fn resume(&self) {
        let _ = self.tx.send(ProcessCommand::Resume);
    }
}

pub struct Worker {
    pub idx: usize,
    cfg: ServiceConfig,
    state: WorkerState,
    handle: reactor::Handle,
    pub events: Events,
    pub restore_from_fail: bool,
    started: Instant,
    restarts: u16,
    cmd: unsync::mpsc::UnboundedSender<ProcessNotification>,
}

impl Worker {

    pub fn new(idx: usize, handle: &reactor::Handle, cfg: ServiceConfig,
               cmd: unsync::mpsc::UnboundedSender<ProcessNotification>) -> Worker
    {

        Worker {
            idx: idx,
            cfg: cfg,
            state: WorkerState::Initial,
            handle: handle.clone(),
            events: Events::new(50),
            started: Instant::now(),
            restore_from_fail: false,
            restarts: 0,
            cmd: cmd}
    }

    pub fn start(&mut self, reason: Reason) {
        let id = self.idx;
        match self.state {
            WorkerState::Initial | WorkerState::Stopped | WorkerState::Failed => {
                debug!("Starting worker process id: {:?}", id);
                let (pid, tx) = Process::start(&self.handle, &self.cfg, self.cmd.clone());
                self.state = WorkerState::Starting(ProcessInfo{pid: pid, tx: tx});
                self.events.add(State::Starting, reason, str(pid));
            }
            _ => (),
        }
    }

    pub fn loaded(&mut self, pid: Pid) {
        let state = std::mem::replace(&mut self.state, WorkerState::Initial);

        match state {
            WorkerState::Starting(p) => {
                if p.pid == pid {
                    self.restarts = 0;
                    p.start();
                    self.events.add(State::Running, Reason::None, str(p.pid));
                    self.state = WorkerState::Running(p);
                    self.restore_from_fail = false;
                } else {
                    self.state = WorkerState::Starting(p);
                }
            }
            WorkerState::Reloading(p, old) => {
                if p.pid == pid {
                    self.restarts = 0;
                    old.stop();
                    p.start();
                    self.events.add(State::StoppingOld, Reason::None, str(old.pid));
                    self.state = WorkerState::StoppingOld(p, old);
                } else {
                    self.state = WorkerState::Reloading(p, old);
                }
            },
            WorkerState::Restarting(p, old) => {
                if p.pid == pid {
                    self.restarts = 0;
                    old.quit();
                    p.start();
                    self.events.add(State::StoppingOld, Reason::None, str(old.pid));
                    self.state = WorkerState::StoppingOld(p, old);
                } else {
                    self.state = WorkerState::Restarting(p, old);
                }
            },
            state => self.state = state
        };
    }

    pub fn is_running(&self) -> bool {
        match self.state {
            WorkerState::Running(_) => true,
            _ => false
        }
    }

    pub fn is_failed(&self) -> bool {
        match self.state {
            WorkerState::Failed => true,
            WorkerState::Running(_) => self.restore_from_fail,
            _ => false
        }
    }

    pub fn is_stopped(&self) -> bool {
        match self.state {
            WorkerState::Stopped => true,
            _ => false
        }
    }

    pub fn reload(&mut self, graceful: bool, reason: Reason) {
        let state = std::mem::replace(&mut self.state, WorkerState::Initial);

        match state {
            WorkerState::Running(process) => {
                // start new worker
                let (pid, tx) = Process::start(&self.handle, &self.cfg, self.cmd.clone());
                let info = ProcessInfo{pid: pid, tx: tx};

                if graceful {
                    info!("Reloading worker: (pid:{})", process.pid);
                    self.events.add(State::Reloading, reason, str(process.pid));
                    self.state = WorkerState::Reloading(info, process);
                } else {
                    info!("Restarting worker: (pid:{})", process.pid);
                    self.events.add(State::Restarting, reason, str(process.pid));
                    self.state = WorkerState::Restarting(info, process);
                }
            },
            WorkerState::Failed | WorkerState::Stopped => {
                self.restarts = 0;
                self.state = WorkerState::Initial;
                self.start(reason);
            },
            _ => self.state = state,
        }
    }

    pub fn stop(&mut self, reason: Reason) {
        let state = std::mem::replace(&mut self.state, WorkerState::Initial);

        match state {
            WorkerState::Initial | WorkerState::Stopped | WorkerState::Failed => {
                self.state = WorkerState::Stopped;
                self.events.add(State::Stopped, reason, None);
            }
            WorkerState::Starting(process) => {
                process.quit();
                self.events.add(State::Stopping, reason, str(process.pid));
                self.state = WorkerState::Stopping(process);
            }
            WorkerState::Stopping(process) =>
                self.state = WorkerState::Stopping(process),
            WorkerState::StoppingOld(process, old_proc) => {
                old_proc.quit();
                process.stop();
                self.events.add(State::Stopping, reason, str(process.pid));
                self.state = WorkerState::Stopping(process);
            }
            WorkerState::Running(process) => {
                process.stop();
                self.events.add(State::Stopping, reason, str(process.pid));
                self.state = WorkerState::Stopping(process);
            }
            WorkerState::Reloading(process, old_proc) => {
                process.quit();
                old_proc.stop();
                self.events.add(State::Stopping, reason, str(old_proc.pid));
                self.state = WorkerState::Stopping(old_proc);
            }
            WorkerState::Restarting(process, old_proc) => {
                process.quit();
                old_proc.stop();
                self.events.add(State::Stopping, reason, str(old_proc.pid));
                self.state = WorkerState::Stopping(old_proc);
            }
        }
    }

    pub fn quit(&mut self, reason: Reason) {
        let state = std::mem::replace(&mut self.state, WorkerState::Initial);

        match state {
            WorkerState::Initial | WorkerState::Stopped | WorkerState::Failed => {
                self.state = WorkerState::Stopped;
                self.events.add(State::Stopped, reason, None);
            }
            WorkerState::Starting(process) => {
                process.quit();
                self.events.add(State::Stopping, reason, str(process.pid));
                self.state = WorkerState::Stopping(process);
            }
            WorkerState::Stopping(process) =>
                self.state = WorkerState::Stopping(process),
            WorkerState::StoppingOld(process, old_proc) => {
                old_proc.quit();
                process.quit();
                self.events.add(State::StoppingOld, reason, str(process.pid));
                self.state = WorkerState::Stopping(process);
            }
            WorkerState::Running(process) => {
                process.quit();
                self.events.add(State::Stopping, reason, str(process.pid));
                self.state = WorkerState::Stopping(process);
            }
            WorkerState::Reloading(process, old_proc) => {
                process.quit();
                old_proc.quit();
                self.events.add(State::Stopping, reason, str(old_proc.pid));
                self.state = WorkerState::Stopping(old_proc);
            }
            WorkerState::Restarting(process, old_proc) => {
                process.quit();
                old_proc.quit();
                self.events.add(State::Stopping, reason, str(old_proc.pid));
                self.state = WorkerState::Stopping(old_proc);
            }
        }
    }

    pub fn message(&mut self, pid: Pid, message: &WorkerMessage) {
        let reload = match self.state {
            WorkerState::Running(ref process) => process.pid == pid,
            _ => false
        };

        if reload {
            match message {
                &WorkerMessage::reload => {
                    self.reload(true, Reason::WorkerRequest)
                },
                &WorkerMessage::restart => {
                    self.reload(false, Reason::WorkerRequest)
                },
                _ => (),
            }
        }
    }

    pub fn pause(&mut self, reason: Reason) {
        match self.state {
            WorkerState::Running(ref process) => {
                process.pause();
                self.events.add(State::Paused, reason, str(process.pid));
            }
            _ => (),
        }
    }

    pub fn resume(&mut self, reason: Reason) {
        match self.state {
            WorkerState::Running(ref process) => {
                process.resume();
                self.events.add(State::Running, reason, str(process.pid));
            }
            _ => (),
        }
    }

    pub fn exited(&mut self, pid: Pid, err: &ProcessError) {
        let state = std::mem::replace(&mut self.state, WorkerState::Initial);

        match state {
            WorkerState::Running(process) => {
                if process.pid != pid {
                    self.state = WorkerState::Running(process);
                } else {
                    match err {
                        &ProcessError::StartupTimeout => {
                            self.state = WorkerState::Running(process);
                            self.events.add(State::Running, err.into(), str(pid));
                            self.restore_from_fail = true;
                            self.reload(false, Reason::ReloadAftreTimeout);
                            return
                        }
                        _ => {
                            // kill worker
                            process.quit();

                            // start new worker
                            self.started = Instant::now();
                            self.state = WorkerState::Initial;
                            self.events.add(State::Stopped, err.into(), str(pid));
                            self.start(Reason::RestartFailedRunningWorker);
                        }
                    }
                }
            }
            WorkerState::Starting(process) => {
                // new process died, need to restart
                if process.pid != pid {
                    self.state = WorkerState::Starting(process);
                } else {
                    match err {
                        // can not boot worker, fail immidietly
                        &ProcessError::InitFailed | &ProcessError::BootFailed => {
                            self.state = WorkerState::Failed;
                            self.events.add(State::Failed, Reason::from(err), str(pid));
                            return
                        }
                        &ProcessError::ExitCode(0) => {
                            // check for fast restart
                            let now = Instant::now();
                            if now.duration_since(self.started) > Duration::new(10, 0) {
                                self.started = now;
                                self.restarts = 0;
                            } else {
                                self.restarts += 1;
                            }
                        }
                        _ =>
                            self.restarts += 1,
                    }

                    self.events.add(State::Failed, Reason::from(err), str(pid));

                    if self.restarts < self.cfg.restarts {
                        // just in case
                        let _ = (&process.tx).send(ProcessCommand::Quit);

                        // start new worker
                        self.state = WorkerState::Initial;
                        self.start(Reason::RestartFailedStartingWorker);
                    } else {
                        error!("Can not start worker (pid:{})", process.pid);
                        self.state = WorkerState::Failed;
                    }
                }
            }
            WorkerState::Reloading(process, old_proc) => {
                // new process died, need to restart
                if process.pid == pid {
                    // can not boot worker, restore old process
                    match err {
                        &ProcessError::InitFailed | &ProcessError::BootFailed => {
                            error!("Can not start worker (pid:{}), restoring old worker",
                                   process.pid);
                            self.restore_from_fail = true;
                            self.events.add(State::ReloadFailed, err.into(), str(pid));
                            self.events.add(State::Running,
                                            Reason::RestoreAftreFailed, str(old_proc.pid));
                            self.state = WorkerState::Running(old_proc);
                            return
                        }
                        &ProcessError::ExitCode(0) => {
                            // check for fast restart
                            let now = Instant::now();
                            if now.duration_since(self.started) > Duration::new(3, 0) {
                                self.started = now;
                                self.restarts = 0;
                            } else {
                                self.restarts += 1;
                            }
                        }
                        _ =>
                            self.restarts += 1,
                    }

                    self.events.add(State::ReloadFailed, err.into(), str(pid));

                    if self.restarts < self.cfg.restarts {
                        // start new worker
                        let (pid, tx) = Process::start(
                            &self.handle, &self.cfg, self.cmd.clone());
                        let info = ProcessInfo{pid: pid, tx: tx};
                        self.state = WorkerState::Reloading(info, old_proc);
                    } else {
                        error!("Can not start worker (pid:{}), restoring old worker",
                               process.pid);
                        self.restore_from_fail = true;
                        self.events.add(State::Running,
                                        Reason::RestoreAftreFailed, str(old_proc.pid));
                        self.state = WorkerState::Running(old_proc);
                    }
                }
                else if old_proc.pid == pid {
                    self.restore_from_fail = false;
                    self.events.add(State::Stopped, Reason::None, str(pid));
                    self.events.add(State::Running, Reason::None, str(process.pid));
                    self.state = WorkerState::Running(process);
                }
                else {
                    self.state = WorkerState::Reloading(process, old_proc);
                }
            }
            WorkerState::Restarting(process, old_proc) => {
                // new process died, need to restart
                if process.pid == pid {
                    // can not boot worker, restore old process
                    match err {
                        &ProcessError::InitFailed | &ProcessError::BootFailed => {
                            error!("Can not start worker (pid:{}), restoring old worker",
                                   process.pid);
                            self.restore_from_fail = true;
                            self.events.add(State::RestartFailed, err.into(), str(pid));
                            self.events.add(State::Running,
                                            Reason::RestoreAftreFailed, str(old_proc.pid));
                            self.state = WorkerState::Running(old_proc);
                            return
                        },
                        &ProcessError::ExitCode(0) => {
                            // check for fast restart
                            let now = Instant::now();
                            if now.duration_since(self.started) > Duration::new(3, 0) {
                                self.started = now;
                                self.restarts = 0;
                            } else {
                                self.restarts += 1;
                            }
                        }
                        _ => {
                            self.restarts += 1;
                        }
                    }

                    self.events.add(State::RestartFailed, err.into(), str(pid));

                    if self.restarts < self.cfg.restarts {
                        // start new worker
                        let (pid, tx) = Process::start(
                            &self.handle, &self.cfg, self.cmd.clone());
                        let info = ProcessInfo{pid: pid, tx: tx};
                        self.state = WorkerState::Restarting(info, old_proc);
                    } else {
                        error!("Can not start worker (pid:{}), restoring old worker",
                               process.pid);
                        self.restore_from_fail = true;
                        self.events.add(
                            State::Running, Reason::RestoreAftreFailed, str(old_proc.pid));
                        self.state = WorkerState::Running(old_proc);
                    }
                }
                else if old_proc.pid == pid {
                    self.restore_from_fail = false;
                    self.events.add(State::Stopped, Reason::None, str(pid));
                    self.events.add(State::Running, Reason::None, str(process.pid));
                    self.state = WorkerState::Running(process);
                } else {
                    self.state = WorkerState::Restarting(process, old_proc);
                }
            }
            WorkerState::StoppingOld(process, old_proc) => {
                // new process died, need to restart
                if process.pid == pid {
                    let _ = (&old_proc.tx).send(ProcessCommand::Quit);
                    self.restarts += 1;
                    self.state = WorkerState::Initial;
                    self.events.add(State::Failed, err.into(), str(pid));
                    self.start(Reason::NewProcessDied);
                }
                else if old_proc.pid == pid {
                    self.restore_from_fail = false;
                    self.events.add(State::Stopped, Reason::None, str(pid));
                    self.events.add(State::Running, Reason::None, str(process.pid));
                    self.state = WorkerState::Running(process);
                } else {
                    self.state = WorkerState::StoppingOld(process, old_proc);
                }
            }
            WorkerState::Stopping(process) => {
                if process.pid == pid {
                    self.state = WorkerState::Stopped;
                    self.events.add(State::Stopped, err.into(), str(pid));
                } else {
                    self.state = WorkerState::Stopping(process);
                }
            },
            state => self.state = state,
        }
    }
}
