use std::io;

use bytes::BytesMut;
use futures::{Async, AsyncSink, Poll, Stream, Sink, StartSend};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::{Encoder, Decoder};
use tokio_io::io::{ReadHalf, WriteHalf};

const INITIAL_CAPACITY: usize = 8 * 1024;
const BACKPRESSURE_BOUNDARY: usize = INITIAL_CAPACITY;

pub trait ActixFramed {

    /// Helper method for splitting this read/write object into two
    /// ActixFramedRead and ActixFramedWrite.
    fn actix_framed<D, E>(self, decoder: D, encoder: E)
                          -> (ActixFramedRead<ReadHalf<Self>, D>,
                              ActixFramedWrite<WriteHalf<Self>, E>)
        where D: Decoder,
              E: Encoder,
              Self: AsyncRead + AsyncWrite + Sized
    {
        let (r, w) = self.split();
        (ActixFramedRead::new(r, decoder), ActixFramedWrite::new(w, encoder))
    }
}

impl<T> ActixFramed for T where T: AsyncRead + AsyncWrite + Sized {}


pub struct ActixFramedRead<T, D> {
    inner: T,
    decoder: D,
    eof: bool,
    is_readable: bool,
    buffer: BytesMut,
}

impl<T, D> ActixFramedRead<T, D>
    where T: AsyncRead,
          D: Decoder,
{
    /// Creates a new `CtxFramedRead` with the given `decoder`.
    pub fn new(inner: T, decoder: D) -> ActixFramedRead<T, D> {
        ActixFramedRead {
            inner: inner,
            decoder: decoder,
            eof: false,
            is_readable: false,
            buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
        }
    }
}

impl<T, D> Stream for ActixFramedRead<T, D>
    where T: AsyncRead,
          D: Decoder,
{
    type Item = D::Item;
    type Error = D::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            // Repeatedly call `decode` or `decode_eof` as long as it is
            // "readable". Readable is defined as not having returned `None`. If
            // the upstream has returned EOF, and the decoder is no longer
            // readable, it can be assumed that the decoder will never become
            // readable again, at which point the stream is terminated.
            if self.is_readable {
                if self.eof {
                    let frame = try!(self.decoder.decode_eof(&mut self.buffer));
                    return Ok(Async::Ready(frame));
                }

                trace!("attempting to decode a frame");

                if let Some(frame) = try!(self.decoder.decode(&mut self.buffer)) {
                    trace!("frame decoded from buffer");
                    return Ok(Async::Ready(Some(frame)));
                }

                self.is_readable = false;
            }

            assert!(!self.eof);

            // Otherwise, try to read more data and try again. Make sure we've
            // got room for at least one byte to read to ensure that we don't
            // get a spurious 0 that looks like EOF
            self.buffer.reserve(1);
            if 0 == try_ready!(self.inner.read_buf(&mut self.buffer)) {
                self.eof = true;
            }

            self.is_readable = true;
        }
    }
}


pub struct ActixFramedWrite<T, E> {
    inner: T,
    encoder: E,
    buffer: BytesMut,
}

impl<T, E> ActixFramedWrite<T, E>
    where T: AsyncWrite,
          E: Encoder,
{
    /// Creates a new `CtxFramedWrite` with the given `encoder`.
    pub fn new(inner: T, encoder: E) -> ActixFramedWrite<T, E> {
        ActixFramedWrite {
            inner: inner,
            encoder: encoder,
            buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
        }
    }
}

impl<T, E> Sink for ActixFramedWrite<T, E>
    where T: AsyncWrite,
          E: Encoder,
{
    type SinkItem = E::Item;
    type SinkError = E::Error;

    fn start_send(&mut self, item: E::Item) -> StartSend<E::Item, E::Error> {
        // If the buffer is already over 8KiB, then attempt to flush it. If after flushing it's
        // *still* over 8KiB, then apply backpressure (reject the send).
        if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
            try!(self.poll_complete());

            if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        try!(self.encoder.encode(item, &mut self.buffer));

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        trace!("flushing framed transport");

        while !self.buffer.is_empty() {
            trace!("writing; remaining={}", self.buffer.len());

            let n = try_nb!(self.inner.write(&self.buffer));

            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero,
                                          "failed to write frame to transport").into());
            }

            // TODO: Add a way to `bytes` to do this w/o returning the drained // data.
            let _ = self.buffer.split_to(n);
        }

        // Try flushing the underlying IO
        try_nb!(self.inner.flush());

        trace!("framed transport flushed");
        return Ok(Async::Ready(()));
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.poll_complete());
        Ok(try!(self.inner.shutdown()))
    }
}
