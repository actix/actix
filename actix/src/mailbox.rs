use std::{fmt, pin::Pin, task, task::Poll};

use futures_core::stream::Stream;

use crate::{
    actor::{Actor, AsyncContext},
    address::{channel, Addr, AddressReceiver, AddressSenderProducer, EnvelopeProxy},
};

/// Default address channel capacity
pub const DEFAULT_CAPACITY: usize = 16;

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

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context, task: &mut task::Context<'_>) {
        #[cfg(feature = "mailbox_assert")]
        let mut n_polls = 0u16;

        while !ctx.waiting() {
            match Pin::new(&mut self.msgs).poll_next(task) {
                Poll::Ready(Some(mut msg)) => {
                    msg.handle(act, ctx);
                    #[cfg(feature = "mailbox_assert")]
                    {
                        n_polls += 1;
                        // Maximum number of consecutive polls in a loop is 256.
                        assert!(n_polls < 256u16, "Too many messages are being processed. Use Self::Context::notify() instead of direct use of address");
                    }
                }
                Poll::Ready(None) | Poll::Pending => return,
            }
        }
    }
}
