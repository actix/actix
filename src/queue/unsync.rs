//! This is copy of unsync/mpsc.rs from https://github.com/alexcrichton/futures-rs
//!
//! A multi-producer, single-consumer, futures-aware, FIFO queue with back
//! pressure, for use communicating between tasks on the same thread.
//!
//! These queues are the same as those in `futures::sync`, except they're not
//! intended to be sent across threads.
#![allow(dead_code)]

use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::mem;
use std::rc::{Rc, Weak};

use futures::task::{self, Task};
use futures::{Async, AsyncSink, Poll, StartSend, Sink, Stream};

/// Creates a bounded in-memory channel with buffered storage.
///
/// This method creates concrete implementations of the `Stream` and `Sink`
/// traits which can be used to communicate a stream of values between tasks
/// with backpressure. The channel capacity is exactly `buffer`. On average,
/// sending a message through this channel performs no dynamic allocation.
pub fn channel<T>(buffer: usize) -> Receiver<T> {
    channel_(Some(buffer))
}

fn channel_<T>(buffer: Option<usize>) -> Receiver<T> {
    let shared = Rc::new(RefCell::new(Shared {
        buffer: VecDeque::new(),
        capacity: buffer,
        blocked_senders: VecDeque::new(),
        blocked_recv: None,
        sender_count: 0,
    }));
    Receiver { state: State::Open(shared) }
}

#[derive(Debug)]
struct Shared<T> {
    buffer: VecDeque<T>,
    capacity: Option<usize>,
    blocked_senders: VecDeque<Task>,
    blocked_recv: Option<Task>,
    // TODO: Redundant to Rc::weak_count; use that if/when stabilized
    sender_count: usize,
}

/// The transmission end of a channel.
///
/// This is created by the `channel` function.
#[derive(Debug)]
pub struct Sender<T> {
    shared: Weak<RefCell<Shared<T>>>,
}

impl<T> Sender<T> {
    fn do_send(&self, msg: T) -> StartSend<T, SendError<T>> {
        let shared = match self.shared.upgrade() {
            Some(shared) => shared,
            None => return Err(SendError(msg)),
        };
        let mut shared = shared.borrow_mut();

        match shared.capacity {
            Some(capacity) if shared.buffer.len() == capacity => {
                shared.blocked_senders.push_back(task::current());
                Ok(AsyncSink::NotReady(msg))
            }
            _ => {
                shared.buffer.push_back(msg);
                if let Some(task) = shared.blocked_recv.take() {
                    drop(shared);
                    task.notify();
                }
                Ok(AsyncSink::Ready)
            }
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        let result = Sender { shared: Weak::clone(&self.shared) };
        if let Some(shared) = self.shared.upgrade() {
            shared.borrow_mut().sender_count += 1;
        }
        result
    }
}

impl<T> Sink for Sender<T> {
    type SinkItem = T;
    type SinkError = SendError<T>;

    fn start_send(&mut self, msg: T) -> StartSend<T, SendError<T>> {
        self.do_send(msg)
    }

    fn poll_complete(&mut self) -> Poll<(), SendError<T>> {
        Ok(Async::Ready(()))
    }

    fn close(&mut self) -> Poll<(), SendError<T>> {
        Ok(Async::Ready(()))
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let shared = match self.shared.upgrade() {
            Some(shared) => shared,
            None => return,
        };
        let mut shared = shared.borrow_mut();
        shared.sender_count -= 1;
        if shared.sender_count == 0 {
            if let Some(task) = shared.blocked_recv.take() {
                // Wake up receiver as its stream has ended
                drop(shared);
                task.notify();
            }
        }
    }
}

/// The receiving end of a channel which implements the `Stream` trait.
///
/// This is created by the `channel` function.
#[derive(Debug)]
pub struct Receiver<T> {
    state: State<T>,
}

/// Possible states of a receiver. We're either Open (can receive more messages)
/// or we're closed with a list of messages we have left to receive.
#[derive(Debug)]
enum State<T> {
    Open(Rc<RefCell<Shared<T>>>),
    Closed(VecDeque<T>),
}

impl<T> Receiver<T> {
    /// Check if receiver connected to senders
    pub fn connected(&mut self) -> bool {
        match self.state {
            State::Open(ref state) => {
                state.borrow().sender_count != 0
            }
            State::Closed(ref deque) => {
                !deque.is_empty()
            }
        }
    }

    /// Get the sender half
    ///
    /// If receiver is not closed, create new Sender
    /// otherwise re-open receiver.
    pub fn sender(&mut self) -> Sender<T> {
        let (sender, items) = match self.state {
            State::Open(ref state) => {
                let sender = Sender { shared: Rc::downgrade(state) };
                state.borrow_mut().sender_count += 1;
                (Some(sender), None)
            }
            State::Closed(ref mut buf) => {
                let items = mem::replace(buf, VecDeque::new());
                (None, Some(items))
            }
        };

        if let Some(items) = items {
            let shared = Rc::new(RefCell::new(Shared {
                buffer: items,
                capacity: None,
                blocked_senders: VecDeque::new(),
                blocked_recv: None,
                sender_count: 1,
            }));
            let sender = Sender { shared: Rc::downgrade(&shared) };
            self.state = State::Open(shared);
            sender
        } else {
            sender.unwrap()
        }
    }

    /// Closes the receiving half
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered.
    pub fn close(&mut self) {
        let (blockers, items) = match self.state {
            State::Open(ref state) => {
                let mut state = state.borrow_mut();
                let items = mem::replace(&mut state.buffer, VecDeque::new());
                let blockers = mem::replace(&mut state.blocked_senders, VecDeque::new());
                (blockers, items)
            }
            State::Closed(_) => return,
        };
        self.state = State::Closed(items);
        for task in blockers {
            task.notify();
        }
    }

    /// Check if the receiving half is closed
    pub fn is_closed(&self) -> bool {
        match self.state {
            State::Closed(_) => true,
            _ => false
        }
    }
}

impl<T> Stream for Receiver<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let me = match self.state {
            State::Open(ref mut me) => me,
            State::Closed(ref mut items) => {
                return Ok(Async::Ready(items.pop_front()))
            }
        };

        if let Some(shared) = Rc::get_mut(me) {
            // All senders have been dropped, so drain the buffer and end the
            // stream.
            return Ok(Async::Ready(shared.borrow_mut().buffer.pop_front()));
        }

        let mut shared = me.borrow_mut();
        if let Some(msg) = shared.buffer.pop_front() {
            if let Some(task) = shared.blocked_senders.pop_front() {
                drop(shared);
                task.notify();
            }
            Ok(Async::Ready(Some(msg)))
        } else {
            shared.blocked_recv = Some(task::current());
            Ok(Async::NotReady)
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.close();
    }
}

/// The transmission end of an unbounded channel.
///
/// This is created by the `unbounded` function.
#[derive(Debug)]
pub struct UnboundedSender<T>(Sender<T>);

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        UnboundedSender(self.0.clone())
    }
}

impl<T> Sink for UnboundedSender<T> {
    type SinkItem = T;
    type SinkError = SendError<T>;

    fn start_send(&mut self, msg: T) -> StartSend<T, SendError<T>> {
        self.0.start_send(msg)
    }
    fn poll_complete(&mut self) -> Poll<(), SendError<T>> {
        Ok(Async::Ready(()))
    }
    fn close(&mut self) -> Poll<(), SendError<T>> {
        Ok(Async::Ready(()))
    }
}

impl<'a, T> Sink for &'a UnboundedSender<T> {
    type SinkItem = T;
    type SinkError = SendError<T>;

    fn start_send(&mut self, msg: T) -> StartSend<T, SendError<T>> {
        self.0.do_send(msg)
    }

    fn poll_complete(&mut self) -> Poll<(), SendError<T>> {
        Ok(Async::Ready(()))
    }

    fn close(&mut self) -> Poll<(), SendError<T>> {
        Ok(Async::Ready(()))
    }
}

impl<T> UnboundedSender<T> {
    /// Sends the provided message along this channel.
    ///
    /// This is an unbounded sender, so this function differs from `Sink::send`
    /// by ensuring the return type reflects that the channel is always ready to
    /// receive messages.
    #[deprecated(note = "renamed to `unbounded_send`")]
    #[doc(hidden)]
    pub fn send(&self, msg: T) -> Result<(), SendError<T>> {
        self.unbounded_send(msg)
    }

    /// Sends the provided message along this channel.
    ///
    /// This is an unbounded sender, so this function differs from `Sink::send`
    /// by ensuring the return type reflects that the channel is always ready to
    /// receive messages.
    pub fn unbounded_send(&self, msg: T) -> Result<(), SendError<T>> {
        let shared = match self.0.shared.upgrade() {
            Some(shared) => shared,
            None => return Err(SendError(msg)),
        };
        let mut shared = shared.borrow_mut();
        shared.buffer.push_back(msg);
        if let Some(task) = shared.blocked_recv.take() {
            drop(shared);
            task.notify();
        }
        Ok(())
    }
}

/// The receiving end of an unbounded channel.
///
/// This is created by the `unbounded` function.
#[derive(Debug)]
pub struct UnboundedReceiver<T>(Receiver<T>);

impl<T> UnboundedReceiver<T> {
    /// Check if receiver connected to senders
    pub fn connected(&mut self) -> bool {
        self.0.connected()
    }

    /// Get the sender half
    pub fn sender(&mut self) -> UnboundedSender<T> {
        UnboundedSender(self.0.sender())
    }

    /// Closes the receiving half
    ///
    /// This prevents any further messages from being sent on the channel while
    /// still enabling the receiver to drain messages that are buffered.
    pub fn close(&mut self) {
        self.0.close();
    }

    /// Check if the receiving half is closed
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

impl<T> Stream for UnboundedReceiver<T> {
    type Item = T;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.0.poll()
    }
}

/// Creates an unbounded in-memory channel with buffered storage.
///
/// Identical semantics to `channel`, except with no limit to buffer size.
pub fn unbounded<T>() -> UnboundedReceiver<T> {
    UnboundedReceiver(channel_(None))
}

/// Error type for sending, used when the receiving end of a channel is
/// dropped
pub struct SendError<T>(T);

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_tuple("SendError")
            .field(&"...")
            .finish()
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "send failed because receiver is gone")
    }
}

impl<T: Any> Error for SendError<T> {
    fn description(&self) -> &str {
        "send failed because receiver is gone"
    }
}

impl<T> SendError<T> {
    /// Returns the message that was attempted to be sent but failed.
    pub fn into_inner(self) -> T {
        self.0
    }
}
