// Execute worker process in child process
use std;
use std::ffi::CString;
use std::io::{Read, Write};
use std::os::unix::io::{RawFd, AsRawFd, FromRawFd};

use libc;
use bytes::{BytesMut, Buf, BufMut, IntoBuf};
use byteorder::BigEndian;
use serde_json as json;
use nix::unistd::{chdir, dup2, execve, setuid, setgid};

use utils;
use worker::{WorkerCommand, WorkerMessage};
use config::ServiceConfig;
use process::{WORKER_INIT_FAILED, WORKER_BOOT_FAILED};


fn send_msg(file: &mut std::fs::File, msg: WorkerMessage) {
    let msg = json::to_string(&msg).unwrap();
    let msg_ref: &[u8] = msg.as_ref();

    let mut buf = BytesMut::with_capacity(msg_ref.len() + 2);
    buf.put_u16::<BigEndian>(msg_ref.len() as u16);
    buf.put(msg_ref);
    if let Err(err) = file.write_all(buf.as_ref()) {
        error!("Failed to notify master: {}", err);
        std::process::exit(WORKER_INIT_FAILED as i32);
    }
}

pub fn exec_worker(cfg: &ServiceConfig, read: RawFd, write: RawFd) {
    // notify master
    let mut file = unsafe{ std::fs::File::from_raw_fd(write) };
    send_msg(&mut file, WorkerMessage::forked);

    // read master response
    let mut buffer = [0; 2];
    let mut file = unsafe{ std::fs::File::from_raw_fd(read) };
    if let Err(err) = file.read_exact(&mut buffer) {
        error!("Failed to read master response: {}", err);
        std::process::exit(WORKER_INIT_FAILED as i32);
    }
    let size = buffer.into_buf().get_u16::<BigEndian>();
    let mut buffer = Vec::with_capacity(size as usize);
    unsafe {buffer.set_len(size as usize)};
    if let Err(err) = file.read_exact(&mut buffer) {
        error!("Failed to read master response: {}", err);
        std::process::exit(WORKER_INIT_FAILED as i32);
    }
    match json::from_slice::<WorkerCommand>(&buffer) {
        Ok(WorkerCommand::prepare) => (),
        Ok(_) | Err(_) => {
            error!("Can not decode master's message: {:?}", &buffer);
            std::process::exit(WORKER_INIT_FAILED as i32);
        }
    }

    // change dir
    if let Some(ref dir) = cfg.directory {
        if let Err(err) = chdir::<str>(dir.as_ref()) {
            error!("Can not change directory {:?} err: {:?}", dir, err);
            send_msg(&mut file, WorkerMessage::cfgerror(
                format!("Can not change directory to {}", dir)));
            std::process::exit(WORKER_INIT_FAILED as i32);
        }
    }

    // set uid
    if let Some(uid) = cfg.uid {
        if let Err(err) = setuid(uid) {
            send_msg(&mut file, WorkerMessage::cfgerror(
                format!("Can not set worker uid, err: {}", err)));
            std::process::exit(WORKER_INIT_FAILED as i32);
        }
    }

    // set gid
    if let Some(gid) = cfg.gid {
        if let Err(err) = setgid(gid) {
            send_msg(&mut file, WorkerMessage::cfgerror(
                format!("Can not set worker gid, err: {}", err)));
            std::process::exit(WORKER_INIT_FAILED as i32);
        }
    }

    // prepare command and arguments
    let mut iter = cfg.command.split_whitespace();
    let path = if let Some(path) = iter.next() {
        if let Some(path) = utils::find_path(&path) {
            path
        } else {
            error!("Can not find executable");
            send_msg(&mut file, WorkerMessage::cfgerror(
                format!("Can not find executable: {}", path)));
            std::process::exit(WORKER_INIT_FAILED as i32);
        }
    } else {
        error!("Can not find executable");
        send_msg(&mut file, WorkerMessage::cfgerror(
            "Can not find executable".to_owned()));
        std::process::exit(WORKER_INIT_FAILED as i32);
    };
    let mut args: Vec<_> = vec![CString::new(path.as_str()).unwrap()];
    args.extend(iter.map(|s| CString::new(s).unwrap()).collect::<Vec<_>>());

    // redirect stdout and stderr
    if let Some(ref stdout) = cfg.stdout {
        match std::fs::OpenOptions::new().append(true).create(true).open(stdout)
        {
            Ok(f) => {
            let _ = dup2(f.as_raw_fd(), libc::STDOUT_FILENO);
        }
            Err(err) => {
                send_msg(&mut file, WorkerMessage::cfgerror(
                    format!("Can open stdout file {}: {}", stdout, err)));
                std::process::exit(WORKER_INIT_FAILED as i32);
            }
        }
    }

    if let Some(ref stderr) = cfg.stderr {
        match std::fs::OpenOptions::new().append(true).create(true).open(stderr)
        {
            Ok(f) => {
                let _ = dup2(f.as_raw_fd(), libc::STDERR_FILENO);

            },
            Err(err) => {
                send_msg(&mut file, WorkerMessage::cfgerror(
                    format!("Can open stderr file {}: {}", stderr, err)));
                std::process::exit(WORKER_INIT_FAILED as i32);
            }
        }
    }

    debug!("Starting worker: {:?}", cfg.command);

    let mut env = utils::get_env_vars(true);
    env.push(CString::new(format!("FECTL_FD={}:{}", read, write)).unwrap());
    env.push(CString::new(format!("FECTL_SRV_NAME={}", cfg.name)).unwrap());
    match execve(&CString::new(path).unwrap(), &args, &env) {
        Ok(_) => unreachable!(),
        Err(err) => {
            error!("Can not execute command: \"{}\" with error: {:?}", cfg.command, err);
            std::process::exit(WORKER_BOOT_FAILED as i32);
        }
    }
}
