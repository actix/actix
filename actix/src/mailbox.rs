use std::{fmt, pin::Pin, task, task::Poll};

use futures_core::stream::Stream;

use crate::{
    actor::{Actor, AsyncContext},
    address::{channel, Addr, AddressReceiver, AddressSenderProducer, EnvelopeProxy},
};

/// Default address channel capacity
pub const DEFAULT_CAPACITY: usize = 16;

/// Maximum number of messages handled in a single mailbox poll before yielding
/// to the event loop.
const MAX_MESSAGES_PER_POLL: usize = 256;

pub struct Mailbox<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    msgs: AddressReceiver<A>,
}

impl<A> fmt::Debug for Mailbox<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Mailbox")
            .field("capacity", &self.capacity())
            .finish()
    }
}

impl<A> Default for Mailbox<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    #[inline]
    fn default() -> Self {
        let (_, rx) = channel::channel(DEFAULT_CAPACITY);
        Mailbox { msgs: rx }
    }
}

impl<A> Mailbox<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    #[inline]
    pub fn new(msgs: AddressReceiver<A>) -> Self {
        Self { msgs }
    }

    pub fn capacity(&self) -> usize {
        self.msgs.capacity()
    }

    pub fn set_capacity(&mut self, cap: usize) {
        self.msgs.set_capacity(cap);
    }

    #[inline]
    pub fn connected(&self) -> bool {
        self.msgs.connected()
    }

    pub fn address(&self) -> Addr<A> {
        Addr::new(self.msgs.sender())
    }

    pub fn sender_producer(&self) -> AddressSenderProducer<A> {
        self.msgs.sender_producer()
    }

    /// Drains the mailbox until it is empty, the actor starts waiting, the
    /// channel closes, or `MAX_MESSAGES_PER_POLL` messages have been handled.
    ///
    /// When the limit is reached the mailbox wakes its task and returns, so the
    /// runtime can poll other tasks before the remaining messages are handled.
    /// This prevents a continuously flooded mailbox from starving timers,
    /// notifications, and actor termination. Context items queued with
    /// `Context::notify` are still polled after the mailbox, so they do not
    /// preempt already queued messages.
    ///
    /// With the `mailbox_assert` feature enabled, reaching the limit panics
    /// instead.
    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context, task: &mut task::Context<'_>) {
        let mut n_polls = 0usize;

        while !ctx.waiting() {
            match Pin::new(&mut self.msgs).poll_next(task) {
                Poll::Ready(Some(mut msg)) => {
                    msg.handle(act, ctx);
                    n_polls += 1;

                    if n_polls >= MAX_MESSAGES_PER_POLL {
                        #[cfg(feature = "mailbox_assert")]
                        panic!("Too many messages are being processed. Use Self::Context::notify() instead of direct use of address");

                        #[cfg(not(feature = "mailbox_assert"))]
                        {
                            // Yield to other tasks, and wake ourselves so the
                            // remaining messages are picked up on the next poll.
                            task.waker().wake_by_ref();
                            return;
                        }
                    }
                }
                Poll::Ready(None) | Poll::Pending => return,
            }
        }
    }
}

#[cfg(all(test, not(feature = "mailbox_assert")))]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        task::{Context as TaskContext, Wake, Waker},
    };

    use super::*;
    use crate::{Actor, Context, Handler, Message};

    struct TestActor {
        handled: Arc<AtomicUsize>,
    }

    impl Actor for TestActor {
        type Context = Context<Self>;
    }

    struct TestMsg;

    impl Message for TestMsg {
        type Result = ();
    }

    impl Handler<TestMsg> for TestActor {
        type Result = ();

        fn handle(&mut self, _: TestMsg, _: &mut Self::Context) -> Self::Result {
            self.handled.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct CountingWaker(AtomicUsize);

    impl Wake for CountingWaker {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn poll_yields_after_max_messages() {
        let (tx, rx) = channel::channel(0);
        let mut mailbox = Mailbox::new(rx);
        let mut ctx = Context::<TestActor>::new();
        let handled = Arc::new(AtomicUsize::new(0));
        let mut act = TestActor {
            handled: Arc::clone(&handled),
        };

        for _ in 0..MAX_MESSAGES_PER_POLL + 10 {
            tx.do_send(TestMsg).unwrap();
        }

        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&counter));
        let mut task = TaskContext::from_waker(&waker);

        mailbox.poll(&mut act, &mut ctx, &mut task);

        assert_eq!(handled.load(Ordering::SeqCst), MAX_MESSAGES_PER_POLL);
        assert_eq!(counter.0.load(Ordering::SeqCst), 1, "task was not woken");

        mailbox.poll(&mut act, &mut ctx, &mut task);

        assert_eq!(handled.load(Ordering::SeqCst), MAX_MESSAGES_PER_POLL + 10);
    }
}
