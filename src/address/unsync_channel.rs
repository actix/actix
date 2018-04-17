//! This is copy of [unsync/mpsc.rs](https://github.com/alexcrichton/futures-rs)
//!
//! A multi-producer, single-consumer, futures-aware, FIFO queue with back
//! pressure, for use communicating between tasks on the same thread.
//!
//! These queues are the same as those in `futures::sync`, except they're not
//! intended to be sent across threads.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use futures::task::{self, Task};
use futures::unsync::oneshot::{channel, Receiver};
use futures::{Async, Poll, Stream};

use super::envelope::UnsyncEnvelope;
use super::{MessageDestinationTransport, SendError, ToEnvelope, Unsync};
use actor::{Actor, AsyncContext};
use handler::{Handler, Message};

pub trait UnsyncSender<M: Message + 'static> {
    fn do_send(&self, msg: M) -> Result<(), SendError<M>>;

    fn try_send(&self, msg: M) -> Result<(), SendError<M>>;

    fn send(&self, msg: M) -> Result<Receiver<M::Result>, SendError<M>>;

    fn boxed(&self) -> Box<UnsyncSender<M>>;
}

struct Shared<A: Actor> {
    buffer: VecDeque<UnsyncEnvelope<A>>,
    capacity: usize,
    blocked_senders: VecDeque<Task>,
    blocked_recv: Option<Task>,
}

/// The transmission end of a channel.
///
/// This is created by the `channel` function.
pub struct UnsyncAddrSender<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    shared: Weak<RefCell<Shared<A>>>,
}

impl<A, M> MessageDestinationTransport<Unsync, A, M> for UnsyncAddrSender<A>
where
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A> + ToEnvelope<Unsync, A, M>,
    M: Message + 'static,
{
    fn send(&self, msg: M) -> Result<Receiver<M::Result>, SendError<M>> {
        UnsyncAddrSender::send(self, msg)
    }
}

impl<A> UnsyncAddrSender<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    pub fn connected(&self) -> bool {
        match self.shared.upgrade() {
            Some(_) => true,
            None => false,
        }
    }

    /// Try to put message to a receiver queue, if queue is full
    /// return message back.
    ///
    /// This method does not register current task in receivers queue.
    pub fn do_send<M>(&self, msg: M) -> Result<(), SendError<M>>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        let shared = match self.shared.upgrade() {
            Some(shared) => shared,
            None => return Err(SendError::Closed(msg)),
        };
        let mut shared = shared.borrow_mut();

        shared
            .buffer
            .push_back(<A::Context as ToEnvelope<Unsync, A, M>>::pack(
                msg,
                None,
            ));
        if let Some(task) = shared.blocked_recv.take() {
            drop(shared);
            task.notify();
        }
        Ok(())
    }

    /// Try to put message to a receiver queue, if queue is full
    /// return message back.
    ///
    /// This method may register current task in receivers queue depends on
    /// state of `park` parameter.
    pub fn try_send<M>(&self, msg: M, park: bool) -> Result<(), SendError<M>>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        let shared = match self.shared.upgrade() {
            Some(shared) => shared,
            None => return Err(SendError::Closed(msg)),
        };
        let mut shared = shared.borrow_mut();

        if shared.capacity == 0 || shared.buffer.len() < shared.capacity {
            shared
                .buffer
                .push_back(<A::Context as ToEnvelope<Unsync, A, M>>::pack(
                    msg,
                    None,
                ));
            if let Some(task) = shared.blocked_recv.take() {
                drop(shared);
                task.notify();
            }
            Ok(())
        } else {
            if park {
                shared.blocked_senders.push_back(task::current());
            }
            Err(SendError::Full(msg))
        }
    }

    /// Try to put message to a receiver queue, if queue is full
    /// return message back.
    ///
    /// This method registers current task in receivers queue.
    pub fn send<M>(&self, msg: M) -> Result<Receiver<M::Result>, SendError<M>>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        let shared = match self.shared.upgrade() {
            Some(shared) => shared,
            None => return Err(SendError::Closed(msg)),
        };
        let mut shared = shared.borrow_mut();

        if shared.capacity == 0 || shared.buffer.len() < shared.capacity {
            let (tx, rx) = channel();
            shared
                .buffer
                .push_back(<A::Context as ToEnvelope<Unsync, A, M>>::pack(
                    msg,
                    Some(tx),
                ));
            if let Some(task) = shared.blocked_recv.take() {
                drop(shared);
                task.notify();
            }
            Ok(rx)
        } else {
            shared.blocked_senders.push_back(task::current());
            Err(SendError::Full(msg))
        }
    }

    /// Get `Sender` for a specific message type
    pub fn into_sender<M>(self) -> Box<UnsyncSender<M>>
    where
        A: Handler<M>,
        M: Message + 'static,
    {
        Box::new(self)
    }
}

impl<A, M> UnsyncSender<M> for UnsyncAddrSender<A>
where
    A: Actor + Handler<M>,
    A::Context: AsyncContext<A>,
    M: Message + 'static,
{
    fn do_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.do_send(msg)
    }
    fn try_send(&self, msg: M) -> Result<(), SendError<M>> {
        self.try_send(msg, true)
    }
    fn send(&self, msg: M) -> Result<Receiver<M::Result>, SendError<M>> {
        self.send(msg)
    }
    fn boxed(&self) -> Box<UnsyncSender<M>> {
        Box::new(self.clone())
    }
}

impl<A> Clone for UnsyncAddrSender<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    fn clone(&self) -> Self {
        UnsyncAddrSender {
            shared: Weak::clone(&self.shared),
        }
    }
}

impl<A> Drop for UnsyncAddrSender<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
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
pub(crate) struct UnsyncAddrReceiver<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    state: Rc<RefCell<Shared<A>>>,
}

impl<A> UnsyncAddrReceiver<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    /// Creates a bounded in-memory channel with buffered storage.
    ///
    /// This method creates concrete implementations of the `Stream`
    /// traits which can be used to communicate a stream of values between tasks
    /// with backpressure. The channel capacity is exactly `cap`. On average,
    /// sending a message through this channel performs no dynamic allocation.
    pub fn new(cap: usize) -> UnsyncAddrReceiver<A> {
        UnsyncAddrReceiver {
            state: Rc::new(RefCell::new(Shared {
                buffer: VecDeque::new(),
                capacity: cap,
                blocked_senders: VecDeque::new(),
                blocked_recv: None,
            })),
        }
    }

    /// Check if receiver connected to senders
    pub fn connected(&self) -> bool {
        Rc::weak_count(&self.state) != 0
    }

    /// Get the sender half
    pub fn sender(&mut self) -> UnsyncAddrSender<A> {
        UnsyncAddrSender {
            shared: Rc::downgrade(&self.state),
        }
    }

    /// Get channel capacity
    pub fn capacity(&self) -> usize {
        self.state.borrow().capacity
    }

    /// Set channel capacity
    ///
    /// This method also wakes up waiting senders
    pub fn set_capacity(&mut self, size: usize) {
        let mut shared = self.state.borrow_mut();
        shared.capacity = size;

        // wake up senders
        if shared.buffer.len() < shared.capacity {
            for _ in 0..shared.capacity - shared.buffer.len() {
                if let Some(task) = shared.blocked_senders.pop_front() {
                    task.notify();
                } else {
                    break;
                }
            }
        }
    }
}

impl<A> Stream for UnsyncAddrReceiver<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    type Item = UnsyncEnvelope<A>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some(shared) = Rc::get_mut(&mut self.state) {
            // All senders have been dropped, so drain the buffer and end the
            // stream.
            return Ok(Async::Ready(shared.borrow_mut().buffer.pop_front()));
        }

        let mut shared = self.state.borrow_mut();
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

impl<A> Drop for UnsyncAddrReceiver<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    fn drop(&mut self) {
        for task in &self.state.borrow().blocked_senders {
            task.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prelude::*;

    struct Act;
    impl Actor for Act {
        type Context = Context<Act>;
    }

    struct Ping;
    impl Message for Ping {
        type Result = ();
    }

    impl Handler<Ping> for Act {
        type Result = ();
        fn handle(&mut self, _: Ping, _: &mut Context<Act>) {}
    }

    #[test]
    fn test_cap() {
        let sys = System::new("test");

        Arbiter::handle().spawn_fn(move || {
            let mut recv = UnsyncAddrReceiver::<Act>::new(1);
            assert_eq!(recv.capacity(), 1);

            let s1 = recv.sender();
            let s2 = recv.sender();

            let _ = s1.send(Ping);
            assert_eq!(recv.state.borrow().buffer.len(), 1);

            let _ = s2.send(Ping);
            assert_eq!(recv.state.borrow().buffer.len(), 1);
            assert_eq!(recv.state.borrow().blocked_senders.len(), 1);

            recv.set_capacity(10);
            assert_eq!(recv.state.borrow().buffer.len(), 1);
            assert_eq!(recv.state.borrow().blocked_senders.len(), 0);

            let _ = s2.send(Ping);
            assert_eq!(recv.state.borrow().buffer.len(), 2);

            Arbiter::system().do_send(actix::msgs::SystemExit(0));
            Ok(())
        });

        sys.run();
    }
}
