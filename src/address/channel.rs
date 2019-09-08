//! This is copy of [sync/mpsc/](https://github.com/alexcrichton/futures-rs)
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering::{Relaxed, SeqCst};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Weak};
use std::{thread, usize};

use futures::sync::oneshot::{channel as sync_channel, Receiver};
use futures::task::{self, Task};
use futures::{Async, Poll, Stream};

use parking_lot::Mutex;

use crate::actor::Actor;
use crate::handler::{Handler, Message};

use super::envelope::{Envelope, ToEnvelope};
use super::queue::{PopResult, Queue};
use super::SendError;

pub trait Sender<M>: Send
where
    M::Result: Send,
    M: Message + Send,
{
    fn do_send(&self, msg: M) -> Result<(), SendError<M>>;

    fn try_send(&self, msg: M) -> Result<(), SendError<M>>;

    fn send(&self, msg: M) -> Result<Receiver<M::Result>, SendError<M>>;

    fn boxed(&self) -> Box<dyn Sender<M>>;

    fn hash(&self) -> usize;

    fn connected(&self) -> bool;
}

/// The transmission end of a channel which is used to send values.
///
/// This is created by the `channel` method.
pub struct AddressSender<A: Actor> {
    // Channel state shared between the sender and receiver.
    inner: Arc<Inner<A>>,

    // Handle to the task that is blocked on this sender. This handle is sent
    // to the receiver half in order to be notified when the sender becomes
    // unblocked.
    sender_task: Arc<Mutex<SenderTask>>,

    // True if the sender might be blocked. This is an optimization to avoid
    // having to lock the mutex most of the time.
    maybe_parked: Arc<AtomicBool>,
}

impl<A: Actor> fmt::Debug for AddressSender<A> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("AddressSender")
            .field("sender_task", &self.sender_task)
            .field("maybe_parked", &self.maybe_parked)
            .finish()
    }
}

/// A weakly referenced version of `AddressSender`.
///
/// This is created by the `AddressSender::downgrade` method.
pub struct WeakAddressSender<A: Actor> {
    inner: Weak<Inner<A>>,
}

impl<A: Actor> fmt::Debug for WeakAddressSender<A> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("WeakAddressSender").finish()
    }
}

trait AssertKinds: Send + Sync + Clone {}

/// The receiving end of a channel which implements the `Stream` trait.
///
/// This is a concrete implementation of a stream which can be used to represent
/// a stream of values being computed elsewhere. This is created by the
/// `channel` method.
pub struct AddressReceiver<A: Actor> {
    inner: Arc<Inner<A>>,
}

/// Generate `AddressSenders` for the channel
pub struct AddressSenderProducer<A: Actor> {
    inner: Arc<Inner<A>>,
}

struct Inner<A: Actor> {
    // Max buffer size of the channel. If `0` then the channel is unbounded.
    buffer: AtomicUsize,

    // Internal channel state. Consists of the number of messages stored in the
    // channel as well as a flag signalling that the channel is closed.
    state: AtomicUsize,

    // Atomic, FIFO queue used to send messages to the receiver.
    message_queue: Queue<Envelope<A>>,

    // Atomic, FIFO queue used to send parked task handles to the receiver.
    parked_queue: Queue<Arc<Mutex<SenderTask>>>,

    // Number of senders in existence.
    num_senders: AtomicUsize,

    // Handle to the receiver's task.
    recv_task: Mutex<ReceiverTask>,
}

// Struct representation of `Inner::state`.
#[derive(Debug, Clone, Copy)]
struct State {
    // `true` when the channel is open
    is_open: bool,

    // Number of messages in the channel
    num_messages: usize,
}

#[derive(Debug)]
struct ReceiverTask {
    unparked: bool,
    task: Option<Task>,
}

// Returned from Receiver::try_park()
enum TryPark {
    Parked,
    NotEmpty,
}

// The `is_open` flag is stored in the left-most bit of `Inner::state`
const OPEN_MASK: usize = usize::MAX - (usize::MAX >> 1);

// When a new channel is created, it is created in the open state with no
// pending messages.
const INIT_STATE: usize = OPEN_MASK;

// The maximum number of messages that a channel can track is `usize::MAX >> 1`
const MAX_CAPACITY: usize = !(OPEN_MASK);

// The maximum requested buffer size must be less than the maximum capacity of
// a channel. This is because each sender gets a guaranteed slot.
const MAX_BUFFER: usize = MAX_CAPACITY >> 1;

// Sent to the consumer to wake up blocked producers
#[derive(Debug)]
struct SenderTask {
    task: Option<Task>,
    is_parked: bool,
}

impl SenderTask {
    fn new() -> Self {
        SenderTask {
            task: None,
            is_parked: false,
        }
    }

    fn notify(&mut self) -> bool {
        self.is_parked = false;

        if let Some(task) = self.task.take() {
            task.notify();
            true
        } else {
            false
        }
    }
}

/// Creates an in-memory channel implementation of the `Stream` trait with
/// bounded capacity.
///
/// This method creates a concrete implementation of the `Stream` trait which
/// can be used to send values across threads in a streaming fashion. This
/// channel is unique in that it implements back pressure to ensure that the
/// sender never outpaces the receiver. The channel capacity is equal to
/// `buffer + num-senders`. In other words, each sender gets a guaranteed slot
/// in the channel capacity, and on top of that there are `buffer` "first come,
/// first serve" slots available to all senders.
///
/// The `Receiver` returned implements the `Stream` trait and has access to any
/// number of the associated combinators for transforming the result.
pub fn channel<A: Actor>(buffer: usize) -> (AddressSender<A>, AddressReceiver<A>) {
    // Check that the requested buffer size does not exceed the maximum buffer
    // size permitted by the system.
    assert!(buffer < MAX_BUFFER, "requested buffer size too large");

    let inner = Arc::new(Inner {
        buffer: AtomicUsize::new(buffer),
        state: AtomicUsize::new(INIT_STATE),
        message_queue: Queue::new(),
        parked_queue: Queue::new(),
        num_senders: AtomicUsize::new(1),
        recv_task: Mutex::new(ReceiverTask {
            unparked: false,
            task: None,
        }),
    });

    let tx = AddressSender {
        inner: Arc::clone(&inner),
        sender_task: Arc::new(Mutex::new(SenderTask::new())),
        maybe_parked: Arc::new(AtomicBool::new(false)),
    };

    let rx = AddressReceiver { inner };

    (tx, rx)
}

//
//
// ===== impl Sender =====
//
//
impl<A: Actor> AddressSender<A> {
    /// Is the channel still open
    pub fn connected(&self) -> bool {
        let curr = self.inner.state.load(SeqCst);
        let state = decode_state(curr);

        state.is_open
    }

    /// Attempts to send a message on this `Sender<A>` with blocking.
    ///
    /// This function must be called from inside of a task.
    pub fn send<M>(&self, msg: M) -> Result<Receiver<M::Result>, SendError<M>>
    where
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
        M::Result: Send,
        M: Message + Send,
    {
        // If the sender is currently blocked, reject the message
        if !self.poll_unparked(false).is_ready() {
            return Err(SendError::Full(msg));
        }

        // First, increment the number of messages contained by the channel.
        // This operation will also atomically determine if the sender task
        // should be parked.
        //
        // None is returned in the case that the channel has been closed by the
        // receiver. This happens when `Receiver::close` is called or the
        // receiver is dropped.
        let park_self = match self.inc_num_messages() {
            Some(park_self) => park_self,
            None => return Err(SendError::Closed(msg)),
        };

        // If the channel has reached capacity, then the sender task needs to
        // be parked. This will send the task handle on the parked task queue.
        if park_self {
            self.park(true);
        }
        let (tx, rx) = sync_channel();
        let env = <A::Context as ToEnvelope<A, M>>::pack(msg, Some(tx));
        self.queue_push_and_signal(env);
        Ok(rx)
    }

    /// Attempts to send a message on this `Sender<A>` without blocking.
    pub fn try_send<M>(&self, msg: M, park: bool) -> Result<(), SendError<M>>
    where
        A: Handler<M>,
        <A as Actor>::Context: ToEnvelope<A, M>,
        M::Result: Send,
        M: Message + Send + 'static,
    {
        // If the sender is currently blocked, reject the message
        if !self.poll_unparked(false).is_ready() {
            return Err(SendError::Full(msg));
        }

        let park_self = match self.inc_num_messages() {
            Some(park_self) => park_self,
            None => return Err(SendError::Closed(msg)),
        };

        if park_self && park {
            self.park(true);
        }
        let env = <A::Context as ToEnvelope<A, M>>::pack(msg, None);
        self.queue_push_and_signal(env);
        Ok(())
    }

    /// Send a message on this `Sender<A>` without blocking.
    ///
    /// This function does not park current task.
    pub fn do_send<M>(&self, msg: M) -> Result<(), SendError<M>>
    where
        A: Handler<M>,
        <A as Actor>::Context: ToEnvelope<A, M>,
        M::Result: Send,
        M: Message + Send,
    {
        if self.inc_num_messages().is_none() {
            Err(SendError::Closed(msg))
        } else {
            // If inc_num_messages returned Some(park_self), then the mailbox is still active.
            // We ignore the boolean (indicating to park and wait) in the Some, and queue the
            // message regardless.
            let env = <A::Context as ToEnvelope<A, M>>::pack(msg, None);
            self.queue_push_and_signal(env);
            Ok(())
        }
    }

    /// Downgrade to `WeakAddressSender` which can later be upgraded
    pub fn downgrade(&self) -> WeakAddressSender<A> {
        WeakAddressSender {
            inner: Arc::downgrade(&self.inner),
        }
    }

    // Push message to the queue and signal to the receiver
    fn queue_push_and_signal(&self, msg: Envelope<A>) {
        // Push the message onto the message queue
        self.inner.message_queue.push(msg);

        // Signal to the receiver that a message has been enqueued. If the
        // receiver is parked, this will unpark the task.
        self.signal();
    }

    // Increment the number of queued messages. Returns if the sender should
    // block.
    fn inc_num_messages(&self) -> Option<bool> {
        let mut curr = self.inner.state.load(SeqCst);
        loop {
            let mut state = decode_state(curr);
            if !state.is_open {
                return None;
            }
            state.num_messages += 1;

            let next = encode_state(&state);
            match self
                .inner
                .state
                .compare_exchange(curr, next, SeqCst, SeqCst)
            {
                Ok(_) => {
                    // receiver is full
                    let buffer = self.inner.buffer.load(Relaxed);
                    let park_self = buffer != 0 && state.num_messages >= buffer;
                    return Some(park_self);
                }
                Err(actual) => curr = actual,
            }
        }
    }

    // Signal to the receiver task that a message has been enqueued
    fn signal(&self) {
        // TODO
        // This logic can probably be improved by guarding the lock with an
        // atomic.
        //
        // Do this step first so that the lock is dropped when
        // `unpark` is called
        let task = {
            let mut recv_task = self.inner.recv_task.lock();

            // If the receiver has already been unparked, then there is nothing
            // more to do
            if recv_task.unparked {
                return;
            }

            // Setting this flag enables the receiving end to detect that
            // an unpark event happened in order to avoid unnecessarily
            // parking.
            recv_task.unparked = true;
            recv_task.task.take()
        };

        if let Some(task) = task {
            task.notify();
        }
    }

    fn park(&self, can_park: bool) {
        // TODO: clean up internal state if the task::current will fail
        let task = if can_park {
            Some(task::current())
        } else {
            None
        };

        {
            let mut sender = self.sender_task.lock();
            sender.task = task;
            sender.is_parked = true;
        }

        // Send handle over queue
        self.inner.parked_queue.push(Arc::clone(&self.sender_task));

        // Check to make sure we weren't closed after we sent our task on the queue
        let state = decode_state(self.inner.state.load(SeqCst));
        self.maybe_parked.store(state.is_open, Relaxed);
    }

    fn poll_unparked(&self, do_park: bool) -> Async<()> {
        // First check the `maybe_parked` variable. This avoids acquiring the
        // lock in most cases
        if self.maybe_parked.load(Relaxed) {
            // Get a lock on the task handle
            let mut task = self.sender_task.lock();

            if !task.is_parked {
                self.maybe_parked.store(false, Relaxed);
                return Async::Ready(());
            }

            // At this point, an unpark request is pending, so there will be an
            // unpark sometime in the future. We just need to make sure that
            // the correct task will be notified.
            //
            // Update the task in case the `Sender` has been moved to another
            // task
            task.task = if do_park { Some(task::current()) } else { None };

            Async::NotReady
        } else {
            Async::Ready(())
        }
    }
}

impl<A, M> Sender<M> for AddressSender<A>
where
    A: Handler<M>,
    A::Context: ToEnvelope<A, M>,
    M::Result: Send,
    M: Message + Send + 'static,
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
    fn boxed(&self) -> Box<dyn Sender<M>> {
        Box::new(self.clone())
    }

    fn hash(&self) -> usize {
        self.inner.as_ref() as *const _ as usize
    }

    fn connected(&self) -> bool {
        self.connected()
    }
}

impl<A: Actor> Clone for AddressSender<A> {
    fn clone(&self) -> AddressSender<A> {
        // Since this atomic op isn't actually guarding any memory and we don't
        // care about any orderings besides the ordering on the single atomic
        // variable, a relaxed ordering is acceptable.
        let mut curr = self.inner.num_senders.load(SeqCst);

        loop {
            // If the maximum number of senders has been reached, then fail
            if curr == self.inner.max_senders() {
                panic!("cannot clone `Sender` -- too many outstanding senders");
            }

            debug_assert!(curr < self.inner.max_senders());

            let next = curr + 1;
            let actual = self.inner.num_senders.compare_and_swap(curr, next, SeqCst);

            // The ABA problem doesn't matter here. We only care that the
            // number of senders never exceeds the maximum.
            if actual == curr {
                return AddressSender {
                    inner: Arc::clone(&self.inner),
                    sender_task: Arc::new(Mutex::new(SenderTask::new())),
                    maybe_parked: Arc::new(AtomicBool::new(false)),
                };
            }

            curr = actual;
        }
    }
}

impl<A: Actor> Drop for AddressSender<A> {
    fn drop(&mut self) {
        // Ordering between variables don't matter here
        let prev = self.inner.num_senders.fetch_sub(1, SeqCst);
        // last sender, notify receiver task
        if prev == 1 {
            self.signal();
        }
    }
}

impl<A: Actor> PartialEq for AddressSender<A> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<A: Actor> Eq for AddressSender<A> {}

impl<A: Actor> Hash for AddressSender<A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.inner.as_ref() as *const Inner<A>).hash(state);
    }
}

//
//
// ===== impl WeakSender =====
//
//
impl<A: Actor> WeakAddressSender<A> {
    /// Attempts to upgrade the `WeakAddressSender<A>` pointer to an [`AddressSender<A>`]
    ///
    /// Returns [`None`] if the actor has since been dropped.
    pub fn upgrade(&self) -> Option<AddressSender<A>> {
        match Weak::upgrade(&self.inner) {
            Some(inner) => Some(AddressSenderProducer { inner }.sender()),
            None => None,
        }
    }
}

//
//
// ===== impl SenderProducer =====
//
//
impl<A: Actor> AddressSenderProducer<A> {
    /// Are any senders connected
    pub fn connected(&self) -> bool {
        self.inner.num_senders.load(SeqCst) != 0
    }

    /// Get channel capacity
    pub fn capacity(&self) -> usize {
        self.inner.buffer.load(Relaxed)
    }

    /// Set channel capacity
    ///
    /// This method wakes up all waiting senders if new capacity is greater
    /// than current
    pub fn set_capacity(&mut self, cap: usize) {
        let buffer = self.inner.buffer.load(Relaxed);
        self.inner.buffer.store(cap, Relaxed);

        // wake up all
        if cap > buffer {
            loop {
                match unsafe { self.inner.parked_queue.pop() } {
                    PopResult::Data(task) => {
                        task.lock().notify();
                    }
                    PopResult::Empty => {
                        // Queue empty, no task to wake up.
                        return;
                    }
                    PopResult::Inconsistent => {
                        // Same as above
                        thread::yield_now();
                    }
                }
            }
        }
    }

    /// Get sender side of the channel
    pub fn sender(&self) -> AddressSender<A> {
        // this code same as Sender::clone
        let mut curr = self.inner.num_senders.load(SeqCst);

        loop {
            // If the maximum number of senders has been reached, then fail
            if curr == self.inner.max_senders() {
                panic!("cannot clone `Sender` -- too many outstanding senders");
            }

            let next = curr + 1;
            let actual = self.inner.num_senders.compare_and_swap(curr, next, SeqCst);

            // The ABA problem doesn't matter here. We only care that the
            // number of senders never exceeds the maximum.
            if actual == curr {
                return AddressSender {
                    inner: Arc::clone(&self.inner),
                    sender_task: Arc::new(Mutex::new(SenderTask::new())),
                    maybe_parked: Arc::new(AtomicBool::new(false)),
                };
            }

            curr = actual;
        }
    }
}

//
//
// ===== impl Receiver =====
//
//
impl<A: Actor> AddressReceiver<A> {
    /// Returns whether any senders are still connected.
    pub fn connected(&self) -> bool {
        self.inner.num_senders.load(SeqCst) != 0
    }

    /// Returns the channel capacity.
    pub fn capacity(&self) -> usize {
        self.inner.buffer.load(Relaxed)
    }

    /// Sets the channel capacity.
    ///
    /// This method wakes up all waiting senders if the new capacity
    /// is greater than the current one.
    pub fn set_capacity(&mut self, cap: usize) {
        let buffer = self.inner.buffer.load(Relaxed);
        self.inner.buffer.store(cap, Relaxed);

        // wake up all
        if cap > buffer {
            loop {
                match unsafe { self.inner.parked_queue.pop() } {
                    PopResult::Data(task) => {
                        task.lock().notify();
                    }
                    PopResult::Empty => {
                        // Queue empty, no task to wake up.
                        return;
                    }
                    PopResult::Inconsistent => {
                        // Same as above
                        thread::yield_now();
                    }
                }
            }
        }
    }

    /// Returns the sender side of the channel.
    pub fn sender(&self) -> AddressSender<A> {
        // this code same as Sender::clone
        let mut curr = self.inner.num_senders.load(SeqCst);

        loop {
            // If the maximum number of senders has been reached, then fail
            if curr == self.inner.max_senders() {
                panic!("cannot clone `Sender` -- too many outstanding senders");
            }

            let next = curr + 1;
            let actual = self.inner.num_senders.compare_and_swap(curr, next, SeqCst);

            // The ABA problem doesn't matter here. We only care that the
            // number of senders never exceeds the maximum.
            if actual == curr {
                return AddressSender {
                    inner: Arc::clone(&self.inner),
                    sender_task: Arc::new(Mutex::new(SenderTask::new())),
                    maybe_parked: Arc::new(AtomicBool::new(false)),
                };
            }

            curr = actual;
        }
    }

    /// Creates the sender producer.
    pub fn sender_producer(&self) -> AddressSenderProducer<A> {
        AddressSenderProducer {
            inner: self.inner.clone(),
        }
    }

    fn next_message(&mut self) -> Async<Option<Envelope<A>>> {
        // Pop off a message
        loop {
            match unsafe { self.inner.message_queue.pop() } {
                PopResult::Data(msg) => {
                    return Async::Ready(Some(msg));
                }
                PopResult::Empty => {
                    // The queue is empty, return NotReady
                    return Async::NotReady;
                }
                PopResult::Inconsistent => {
                    // Inconsistent means that there will be a message to pop
                    // in a short time. This branch can only be reached if
                    // values are being produced from another thread, so there
                    // are a few ways that we can deal with this:
                    //
                    // 1) Spin
                    // 2) thread::yield_now()
                    // 3) task::current().unwrap() & return NotReady
                    //
                    // For now, thread::yield_now() is used, but it would
                    // probably be better to spin a few times then yield.
                    thread::yield_now();
                }
            }
        }
    }

    // Unpark a single task handle if there is one pending in the parked queue
    fn unpark_one(&mut self) {
        loop {
            match unsafe { self.inner.parked_queue.pop() } {
                PopResult::Data(task) => {
                    if task.lock().notify() {
                        return;
                    }
                }
                PopResult::Empty => {
                    // Queue empty, no task to wake up.
                    return;
                }
                PopResult::Inconsistent => {
                    // Same as above
                    thread::yield_now();
                }
            }
        }
    }

    // Try to park the receiver task
    fn try_park(&self) -> TryPark {
        // First, track the task in the `recv_task` slot
        let mut recv_task = self.inner.recv_task.lock();

        if recv_task.unparked {
            // Consume the `unpark` signal without actually parking
            recv_task.unparked = false;
            return TryPark::NotEmpty;
        }

        recv_task.task = Some(task::current());
        TryPark::Parked
    }

    fn dec_num_messages(&self) {
        let mut curr = self.inner.state.load(SeqCst);

        loop {
            let mut state = decode_state(curr);

            state.num_messages -= 1;

            let next = encode_state(&state);
            match self
                .inner
                .state
                .compare_exchange(curr, next, SeqCst, SeqCst)
            {
                Ok(_) => break,
                Err(actual) => curr = actual,
            }
        }
    }
}

impl<A: Actor> Stream for AddressReceiver<A> {
    type Item = Envelope<A>;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            // Try to read a message off of the message queue.
            let msg = match self.next_message() {
                Async::Ready(msg) => msg,
                Async::NotReady => {
                    // There are no messages to read, in this case, attempt to
                    // park. The act of parking will verify that the channel is
                    // still empty after the park operation has completed.
                    match self.try_park() {
                        TryPark::Parked => {
                            // The task was parked, and the channel is still
                            // empty, return NotReady.
                            return Ok(Async::NotReady);
                        }
                        TryPark::NotEmpty => {
                            // A message has been sent while attempting to
                            // park. Loop again, the next iteration is
                            // guaranteed to get the message.
                            continue;
                        }
                    }
                }
            };

            // If there are any parked task handles in the parked queue, pop
            // one and unpark it.
            self.unpark_one();

            // Decrement number of messages
            self.dec_num_messages();

            // Return the message
            return Ok(Async::Ready(msg));
        }
    }
}

impl<A: Actor> Drop for AddressReceiver<A> {
    fn drop(&mut self) {
        // close
        let mut curr = self.inner.state.load(SeqCst);
        loop {
            let mut state = decode_state(curr);
            if !state.is_open {
                break;
            }
            state.is_open = false;

            let next = encode_state(&state);
            match self
                .inner
                .state
                .compare_exchange(curr, next, SeqCst, SeqCst)
            {
                Ok(_) => break,
                Err(actual) => curr = actual,
            }
        }

        // Wake up any threads waiting as they'll see that we've closed the
        // channel and will continue on their merry way.
        loop {
            match unsafe { self.inner.parked_queue.pop() } {
                PopResult::Data(task) => {
                    task.lock().notify();
                }
                PopResult::Empty => break,
                PopResult::Inconsistent => thread::yield_now(),
            }
        }

        // Drain the channel of all pending messages
        while self.next_message().is_ready() {
            // ...
        }
    }
}

//
//
// ===== impl Inner =====
//
//
impl<A: Actor> Inner<A> {
    // The return value is such that the total number of messages that can be
    // enqueued into the channel will never exceed MAX_CAPACITY
    fn max_senders(&self) -> usize {
        MAX_CAPACITY - self.buffer.load(Relaxed)
    }
}

unsafe impl<A: Actor> Send for Inner<A> {}
unsafe impl<A: Actor> Sync for Inner<A> {}

//
//
// ===== Helpers =====
//
//
fn decode_state(num: usize) -> State {
    State {
        is_open: num & OPEN_MASK == OPEN_MASK,
        num_messages: num & MAX_CAPACITY,
    }
}

fn encode_state(state: &State) -> usize {
    let mut num = state.num_messages;

    if state.is_open {
        num |= OPEN_MASK;
    }

    num
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use std::{thread, time};

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
        System::run(|| {
            let (s1, mut recv) = channel::<Act>(1);
            let s2 = recv.sender();

            let arb = Arbiter::new();
            arb.exec_fn(move || {
                let _ = s1.send(Ping);
            });
            thread::sleep(time::Duration::from_millis(100));
            let arb2 = Arbiter::new();
            arb2.exec_fn(move || {
                let _ = s2.send(Ping);
                let _ = s2.send(Ping);
            });

            thread::sleep(time::Duration::from_millis(100));
            let state = decode_state(recv.inner.state.load(SeqCst));
            assert_eq!(state.num_messages, 2);

            let p = loop {
                match unsafe { recv.inner.parked_queue.pop() } {
                    PopResult::Data(task) => break Some(task),
                    PopResult::Empty => break None,
                    PopResult::Inconsistent => thread::yield_now(),
                }
            };

            assert!(p.is_some());
            recv.inner.parked_queue.push(p.unwrap());

            recv.set_capacity(10);

            thread::sleep(time::Duration::from_millis(100));
            let state = decode_state(recv.inner.state.load(SeqCst));
            assert_eq!(state.num_messages, 2);

            let p = loop {
                match unsafe { recv.inner.parked_queue.pop() } {
                    PopResult::Data(task) => break Some(task),
                    PopResult::Empty => break None,
                    PopResult::Inconsistent => thread::yield_now(),
                }
            };
            assert!(p.is_none());

            System::current().stop();
        })
        .unwrap();
    }
}
