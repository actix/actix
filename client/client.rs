use std::thread;
use std::io::{self, Read, Write};
use std::time::Duration;
use std::os::unix::net::UnixStream;

use serde_json as json;
use byteorder::{BigEndian, ByteOrder};
use bytes::{BufMut, BytesMut};
use tokio_io::codec::{Encoder, Decoder};

use {MasterRequest, MasterResponse};

/// Console commands
#[derive(Clone, Debug)]
pub enum ClientCommand {
    Start(String),
    Pause(String),
    Resume(String),
    Reload(String),
    Restart(String),
    Stop(String),
    Status(String),
    Pid,
    Quit,
    Version,
}

/// Send command to master
pub fn send_command(stream: &mut UnixStream, req: MasterRequest) -> Result<(), io::Error> {
    let mut buf = BytesMut::new();
     ClientTransportCodec.encode(req, &mut buf)?;

    stream.write_all(buf.as_ref())
}

/// read master response
pub fn read_response(stream: &mut UnixStream, buf: &mut BytesMut)
                     -> Result<MasterResponse, io::Error>
{
    loop {
        buf.reserve(1024);

        unsafe {
            match stream.read(buf.bytes_mut()) {
                Ok(n) => {
                    buf.advance_mut(n);

                    if let Some(resp) = ClientTransportCodec.decode(buf)? {
                        return Ok(resp)
                    } else {
                        if n == 0 {
                            return Err(io::Error::new(io::ErrorKind::Other, "closed"))
                        }
                    }
                },
                Err(e) => return Err(e),
            }
        }
    }
}

fn try_read_response(stream: &mut UnixStream, buf: &mut BytesMut)
                     -> Result<MasterResponse, io::Error>
{
    let mut retry = 5;
    loop {
        match read_response(stream, buf) {
            Ok(resp) => {
                debug!("Master response: {:?}", resp);
                return Ok(resp);
            }
            Err(err) => match err.kind() {
                io::ErrorKind::TimedOut =>
                    if retry > 0 {
                        retry -= 1;
                        continue
                    }
                io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                    continue
                }
                _ => return Err(err)
            }
        }
    }
}

/// Run client command
pub fn run(cmd: ClientCommand, sock: String) -> bool {
    // create commands listener and also check if service process is running
    let mut buf = BytesMut::new();
    let mut stream = match UnixStream::connect(&sock) {
        Ok(mut conn) => {
            conn.set_read_timeout(Some(Duration::new(1, 0))).expect("Couldn't set read timeout");
            let _ = send_command(&mut conn, MasterRequest::Ping);

            if let Ok(_) = try_read_response(&mut conn, &mut buf) {
                conn
            } else {
                error!("Master process is not responding.");
                return false
            }
        }
        Err(err) => {
            match err.kind() {
                io::ErrorKind::PermissionDenied => {
                    error!("Can not connect to master. Permission denied. {}", sock);
                },
                _ => {
                    error!("Can not connect to master {}: {}", sock, err);
                }
            }
            return false
        }
    };

    // Send command
    let res = match cmd.clone() {
        ClientCommand::Status(name) =>
            send_command(&mut stream, MasterRequest::Status(name)),
        ClientCommand::Pause(name) => {
            println!("Pause `{}` service.", name);
            send_command(&mut stream, MasterRequest::Pause(name))
        }
        ClientCommand::Resume(name) => {
            println!("Resume `{}` service.", name);
            send_command(&mut stream, MasterRequest::Resume(name))
        }
        ClientCommand::Start(name) => {
            print!("Starting `{}` service.", name);
            send_command(&mut stream, MasterRequest::Start(name))
        }
        ClientCommand::Reload(name) => {
            print!("Reloading `{}` service.", name);
            send_command(&mut stream, MasterRequest::Reload(name))
        }
        ClientCommand::Restart(name) => {
            print!("Restarting `{}` service", name);
            send_command(&mut stream, MasterRequest::Restart(name))
        }
        ClientCommand::Stop(name) => {
            print!("Stopping `{}` service.", name);
            send_command(&mut stream, MasterRequest::Stop(name))
        }
        ClientCommand::Pid => {
            send_command(&mut stream, MasterRequest::Pid)
        }
        ClientCommand::Version => {
            send_command(&mut stream, MasterRequest::Version)
        }
        ClientCommand::Quit => {
            print!("Quiting.");
            send_command(&mut stream, MasterRequest::Quit)
        }
    };
    let _ = io::stdout().flush();

    if let Err(err) = res {
        error!("Can not send command {:?} error: {}", cmd, err);
        return false
    }

    // read response
    loop {
        match try_read_response(&mut stream, &mut buf) {
            Ok(MasterResponse::Pong) => {
                print!(".");
                let _ = io::stdout().flush();
            }
            Ok(MasterResponse::Done) => {
                println!("");
                return true
            }
            Ok(MasterResponse::Pid(pid)) => {
                println!("{}", pid);
                return true
            }
            Ok(MasterResponse::Version(ver)) => {
                println!("{}", ver);
                return true
            }
            Ok(MasterResponse::ServiceStarted) => {
                println!("done");
                return true
            }
            Ok(MasterResponse::ServiceStopped) => {
                println!("done");
                return true
            }
            Ok(MasterResponse::ServiceStatus(s)) => {
                println!("Service status: {}", s);
                return true
            }
            Ok(MasterResponse::ServiceFailed) => {
                println!("failed.");
                return false
            },
            Ok(MasterResponse::ErrorNotReady) => {
                error!("Service is loading");
                return false
            }
            Ok(MasterResponse::ErrorUnknownService) => {
                error!("Service is unknown");
                return false
            }
            Ok(MasterResponse::ErrorServiceStarting) => {
                error!("Service is starting");
                return false
            }
            Ok(MasterResponse::ErrorServiceReloading) => {
                error!("Service is restarting");
                return false
            }
            Ok(MasterResponse::ErrorServiceStopping) => {
                error!("Service is stopping");
                return false
            }
            Ok(resp) => println!("MSG: {:?}", resp),
            Err(err) => {
                println!("Error: {:?}", err);
                error!("Master process is not responding.");
                return false
            }
        }
    }
}

pub struct ClientTransportCodec;

impl Encoder for ClientTransportCodec
{
    type Item = MasterRequest;
    type Error = io::Error;

    fn encode(&mut self, msg: MasterRequest, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = json::to_string(&msg).unwrap();
        let msg_ref: &[u8] = msg.as_ref();

        dst.reserve(msg_ref.len() + 2);
        dst.put_u16::<BigEndian>(msg_ref.len() as u16);
        dst.put(msg_ref);

        Ok(())
    }
}

impl Decoder for ClientTransportCodec
{
    type Item = MasterResponse;
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
            Ok(Some(json::from_slice::<MasterResponse>(&buf)?))
        } else {
            Ok(None)
        }
    }
}
