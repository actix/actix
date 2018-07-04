use futures::{Async, Stream};

use actor::{Actor, AsyncContext};
use address::EnvelopeProxy;
use address::{channel, Addr, AddressReceiver, AddressSenderProducer};

/// Maximum number of consecutive polls in a loop
const MAX_SYNC_POLLS: u32 = 256;

/// Default address channel capacity
pub const DEFAULT_CAPACITY: usize = 16;

pub struct Mailbox<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    msgs: AddressReceiver<A>,
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

struct NumPolls(u32);

impl NumPolls {
    fn inc(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

impl<A> Mailbox<A>
where
    A: Actor,
    A::Context: AsyncContext<A>,
{
    #[inline]
    pub fn new(msgs: AddressReceiver<A>) -> Self {
        Mailbox { msgs }
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

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context) {
        let mut n_polls = NumPolls(0);
        loop {
            let mut not_ready = true;

            // sync messages
            loop {
                if ctx.waiting() {
                    return;
                }

                match self.msgs.poll() {
                    Ok(Async::Ready(Some(mut msg))) => {
                        not_ready = false;
                        msg.handle(act, ctx);
                    }
                    Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => break,
                }
                debug_assert!(
                    n_polls.inc() < MAX_SYNC_POLLS,
                    "Use Self::Context::notify() instead of direct use of address"
                );
            }

            if not_ready {
                return;
            }
        }
    }
}
