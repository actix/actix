use futures::{Async, Stream};

use actor::{Actor, AsyncContext};
use address::{sync_channel, Addr, Syn, SyncAddressReceiver, Unsync, UnsyncAddrReceiver};
use address::EnvelopeProxy;

/// Maximum number of consecutive polls in a loop
const MAX_SYNC_POLLS: u32 = 256;

/// Default address channel capacity
pub const DEFAULT_CAPACITY: usize = 16;


pub(crate) struct Mailbox<A> where A: Actor, A::Context: AsyncContext<A> {
    sync_msgs: Option<SyncAddressReceiver<A>>,
    unsync_msgs: UnsyncAddrReceiver<A>,
}

impl<A> Default for Mailbox<A> where A: Actor, A::Context: AsyncContext<A> {

    #[inline]
    fn default() -> Self {
        Mailbox {
            sync_msgs: None,
            unsync_msgs: UnsyncAddrReceiver::new(DEFAULT_CAPACITY) }
    }
}

struct NumPolls(u32);

impl NumPolls {
    fn inc(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

impl<A> Mailbox<A> where A: Actor, A::Context: AsyncContext<A>
{
    #[inline]
    pub fn new(rx: SyncAddressReceiver<A>) -> Self {
        Mailbox {
            sync_msgs: Some(rx),
            unsync_msgs: UnsyncAddrReceiver::new(16) }
    }

    pub fn capacity(&self) -> usize {
        self.unsync_msgs.capacity()
    }

    pub fn set_capacity(&mut self, cap: usize) {
        self.unsync_msgs.set_capacity(cap);
        self.sync_msgs.as_mut().map(|msgs| msgs.set_capacity(cap));
    }
    
    #[inline]
    pub fn connected(&self) -> bool {
        self.unsync_msgs.connected() ||
            self.sync_msgs.as_ref().map(|msgs| msgs.connected()).unwrap_or(false)
    }

    pub fn remote_address(&mut self) -> Addr<Syn,A> {
        if self.sync_msgs.is_none() {
            let (tx, rx) = sync_channel::channel(self.unsync_msgs.capacity());
            self.sync_msgs = Some(rx);
            Addr::new(tx)
        } else {
            if let Some(ref mut addr) = self.sync_msgs {
                return Addr::new(addr.sender())
            }
            unreachable!();
        }
    }

    #[inline]
    pub fn unsync_address(&mut self) -> Addr<Unsync, A> {
        Addr::new(self.unsync_msgs.sender())
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context) {
        let mut n_polls = NumPolls(0);
        loop {
            let mut not_ready = true;

            // unsync messages
            loop {
                if ctx.waiting() { return }

                match self.unsync_msgs.poll() {
                    Ok(Async::Ready(Some(mut msg))) => {
                        not_ready = false;
                        msg.handle(act, ctx);
                    }
                    Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => break,
                }
                debug_assert!(n_polls.inc() < MAX_SYNC_POLLS,
                              "Use Self::Context::notify() instead of direct use of address");
            }

            // sync messages
            if let Some(ref mut msgs) = self.sync_msgs {
                loop {
                    if ctx.waiting() { return }

                    match msgs.poll() {
                        Ok(Async::Ready(Some(mut msg))) => {
                            not_ready = false;
                            msg.handle(act, ctx);
                        }
                        Ok(Async::Ready(None)) | Ok(Async::NotReady) | Err(_) => break,
                    }
                    debug_assert!(n_polls.inc() < MAX_SYNC_POLLS,
                                  "Use Self::Context::notify() instead of direct use of address");
                }
            }

            if not_ready {
                return
            }
        }
    }
}
