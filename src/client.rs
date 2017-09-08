use std::thread;
use std::io::{self, Read, Write};
use std::time::Duration;
use std::os::unix::net::UnixStream;

use serde_json as json;
use byteorder::{BigEndian, ByteOrder};
use bytes::{BufMut, BytesMut};
use tokio_io::codec::{Encoder, Decoder};

use config::MasterConfig;
use master_types::{MasterRequest, MasterResponse};

/// Master alive status
pub enum AliveStatus {
    /// Master process is alive
    Alive,
    /// Master process is not alive
    NotAlive,
    /// Unix socket is connected but master process does not respond
    NotResponding,
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

/// Check if master process is alive. Try to connect over unix socket
/// and send `Ping` command
pub fn is_alive(cfg: &MasterConfig) -> AliveStatus {
    match UnixStream::connect(&cfg.sock) {
        Ok(mut conn) => {
            conn.set_read_timeout(Some(Duration::new(1, 0))).expect("Couldn't set read timeout");
            let _ = send_command(&mut conn, MasterRequest::Ping);

            if let Ok(_) = try_read_response(&mut conn, &mut BytesMut::new()) {
                AliveStatus::Alive
            } else {
                AliveStatus::NotResponding
            }
        }
        Err(_) => {
            AliveStatus::NotAlive
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
