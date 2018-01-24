//! This is copy of [unsync/mpsc.rs](https://github.com/alexcrichton/futures-rs)
//!
//! A multi-producer, single-consumer, futures-aware, FIFO queue with back
//! pressure, for use communicating between tasks on the same thread.
//!
//! These queues are the same as those in `futures::sync`, except they're not
//! intended to be sent across threads.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::mem;
use std::rc::{Rc, Weak};

use futures::task::{self, Task};
use futures::{Async, AsyncSink, Poll, StartSend, Stream};

use queue::{Either, MessageOption};

/// Creates a bounded in-memory channel with buffered storage.
///
/// This method creates concrete implementations of the `Stream` and `Sink`
/// traits which can be used to communicate a stream of values between tasks
/// with backpressure. The channel capacity is exactly `buffer`. On average,
/// sending a message through this channel performs no dynamic allocation.
pub fn channel<T>(buffer: usize) -> Receiver<T> {
    if buffer == 0 {
        channel_(None)
    } else {
        channel_(Some(buffer))
    }
}

fn channel_<T>(buffer: Option<usize>) -> Receiver<T> {
    let shared = Rc::new(RefCell::new(Shared {
        buffer: VecDeque::new(),
        capacity: buffer,
        blocked_senders: VecDeque::new(),
        blocked_recv: None,
    }));
    Receiver { state: State::Open(shared) }
}

#[derive(Debug)]
struct Shared<T> {
    buffer: VecDeque<T>,
    capacity: Option<usize>,
    blocked_senders: VecDeque<Task>,
    blocked_recv: Option<Task>,
}

/// The transmission end of a channel.
///
/// This is created by the `channel` function.
#[derive(Debug)]
pub struct Sender<T> {
    shared: Weak<RefCell<Shared<T>>>,
}

impl<T> Sender<T> {
    pub fn connected(&self) -> bool {
        match self.shared.upgrade() {
            Some(_) => true,
            None => false,
        }
    }

    pub fn send<F, M>(&self, f: F) -> StartSend<M, M>
        where F: FnOnce(MessageOption) -> Either<T, M>
    {
        let shared = match self.shared.upgrade() {
            Some(shared) => shared,
            None => return Err(f(MessageOption::Message).unwrap_message()),
        };
        let mut shared = shared.borrow_mut();

        match shared.capacity {
            Some(capacity) if shared.buffer.len() == capacity => {
                shared.blocked_senders.push_back(task::current());
                Ok(AsyncSink::NotReady(f(MessageOption::Message).unwrap_message()))
            }
            _ => {
                shared.buffer.push_back(f(MessageOption::Envelope).unwrap_envelope());
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
        Sender { shared: Weak::clone(&self.shared) }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let shared = match self.shared.upgrade() {
            Some(shared) => shared,
            None => return,
        };
        if Rc::weak_count(&shared) == 1 {
            let task = { shared.borrow_mut().blocked_recv.take() };
            if let Some(task) = task {
                // Wake up receiver as its stream has ended
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
    pub fn connected(&self) -> bool {
        match self.state {
            State::Open(ref state) => {
                Rc::weak_count(state) != 0
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
                (Some(Sender { shared: Rc::downgrade(state) }), None)
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
