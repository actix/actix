use futures::unsync::oneshot::Sender;

mod address;
mod channel;
mod message;
mod envelope;

pub use self::address::LocalAddress;
pub use self::message::{LocalRequest, LocalFutRequest, UpgradeAddress};

pub(crate) use self::channel::LocalAddrReceiver;

use actor::Actor;
use address::Address;
use self::envelope::LocalEnvelope;

/// context protocol
pub(crate) enum LocalAddrProtocol<A: Actor> {
    /// message envelope
    Envelope(LocalEnvelope<A>),
    /// Request remote address
    Upgrade(Sender<Address<A>>),
}
