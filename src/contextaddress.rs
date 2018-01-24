use futures::{Async, Stream};

use actor::{Actor, AsyncContext};
use address::{Address, LocalAddress};
use context::ContextProtocol;
use envelope::Envelope;
use queue::{sync, unsync};

/// Maximum number of consecutive polls in a loop
const MAX_SYNC_POLLS: u32 = 256;

pub(crate) struct ContextAddress<A> where A: Actor {
    sync_msgs: Option<sync::UnboundedReceiver<Envelope<A>>>,
    unsync_msgs: unsync::UnboundedReceiver<ContextProtocol<A>>,
}

impl<A> Default for ContextAddress<A> where A: Actor {

    #[inline]
    fn default() -> Self {
        ContextAddress {
            sync_msgs: None,
            unsync_msgs: unsync::unbounded() }
    }
}

struct NumPolls(u32);

impl NumPolls {
    fn inc(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

impl<A> ContextAddress<A> where A: Actor, A::Context: AsyncContext<A>
{
    #[inline]
    pub fn new(rx: sync::UnboundedReceiver<Envelope<A>>) -> Self {
        ContextAddress {
            sync_msgs: Some(rx),
            unsync_msgs: unsync::unbounded() }
    }

    #[inline]
    pub fn connected(&self) -> bool {
        self.unsync_msgs.connected() ||
            self.sync_msgs.as_ref().map(|msgs| msgs.connected()).unwrap_or(false)
    }

    pub fn remote_address(&mut self) -> Address<A> {
        if self.sync_msgs.is_none() {
            let (tx, rx) = sync::unbounded();
            self.sync_msgs = Some(rx);
            Address::new(tx)
        } else {
            if let Some(ref mut addr) = self.sync_msgs {
                return Address::new(addr.sender())
            }
            unreachable!();
        }
    }

    #[inline]
    pub fn local_address(&mut self) -> LocalAddress<A> {
        LocalAddress::new(self.unsync_msgs.sender())
    }

    pub fn poll(&mut self, act: &mut A, ctx: &mut A::Context) {
        let mut n_polls = NumPolls(0);
        loop {
            let mut not_ready = true;

            // unsync messages
            loop {
                if ctx.waiting() { return }

                match self.unsync_msgs.poll() {
                    Ok(Async::Ready(Some(msg))) => {
                        not_ready = false;
                        match msg {
                            ContextProtocol::Envelope(mut env) => {
                                env.handle(act, ctx)
                            }
                            ContextProtocol::Upgrade(tx) => {
                                let _ = tx.send(self.remote_address());
                            }
                        }
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
