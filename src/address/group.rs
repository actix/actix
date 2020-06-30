use std::hash::Hash;
use std::ops::Deref;

use crate::actor::Actor;
use crate::address::envelope::ToEnvelope;
use crate::address::{Addr, SendError, WeakAddr};
use crate::handler::{Handler, Message};

use weak_table::{traits::WeakElement, PtrWeakHashSet};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct WeakTableAddrWrap<A: Actor>(Addr<A>);

impl<A: Actor> Deref for WeakTableAddrWrap<A> {
    type Target = Addr<A>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
struct WeakTableWeakAddrWrap<A: Actor>(WeakAddr<A>);

impl<A: Actor> Deref for WeakTableWeakAddrWrap<A> {
    type Target = WeakAddr<A>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<A: Actor> WeakElement for WeakTableWeakAddrWrap<A> {
    type Strong = WeakTableAddrWrap<A>;

    fn new(view: &Self::Strong) -> Self {
        WeakTableWeakAddrWrap(view.0.downgrade())
    }

    fn view(&self) -> Option<Self::Strong> {
        self.upgrade().map(|addr| WeakTableAddrWrap(addr))
    }
}

impl<A: Actor> Clone for WeakTableWeakAddrWrap<A> {
    fn clone(&self) -> WeakTableWeakAddrWrap<A> {
        WeakTableWeakAddrWrap(self.0.clone())
    }
}

/// A group of actors that doesn't extend the actors lifetime. This uses `WeakAddrs` inside.
///
/// You can use this to send messages to a group of actors (at least to the ones which are still alive).
#[derive(Debug)]
pub struct GroupWeakAddr<A: Actor> {
    set: PtrWeakHashSet<WeakTableWeakAddrWrap<A>>,
}

impl<A: Actor> Clone for GroupWeakAddr<A> {
    fn clone(&self) -> GroupWeakAddr<A> {
        GroupWeakAddr {
            set: self.set.clone(),
        }
    }
}

pub struct GroupWeakAddrIterator<'a, A: Actor>(
    weak_table::ptr_weak_hash_set::Iter<'a, WeakTableWeakAddrWrap<A>>,
);

impl<'a, A: Actor> core::iter::Iterator for GroupWeakAddrIterator<'a, A> {
    type Item = Addr<A>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|weakwrapper| weakwrapper.deref().clone())
    }
}

impl<'a, A: Actor> IntoIterator for &'a GroupWeakAddr<A> {
    type Item = Addr<A>;
    type IntoIter = GroupWeakAddrIterator<'a, A>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<A: Actor> PartialEq for GroupWeakAddr<A> {
    fn eq(&self, other: &Self) -> bool {
        self.set == other.set
    }
}

impl<A: Actor> Eq for GroupWeakAddr<A> {}

impl<A: Actor> GroupWeakAddr<A> {
    pub fn new() -> Self {
        GroupWeakAddr {
            set: weak_table::PtrWeakHashSet::new(),
        }
    }

    /// Get an iterator for the elements.
    pub fn iter<'a>(&'a self) -> GroupWeakAddrIterator<'a, A> {
        GroupWeakAddrIterator(self.set.iter())
    }

    /// Removes all actors which have expired.
    pub fn remove_expired(&mut self) {
        self.set.remove_expired()
    }

    /// Returns the number of elements the group can hold without reallocating.
    pub fn capacity(&self) -> usize {
        self.set.capacity()
    }

    /// Reserves room for additional elements.
    pub fn reserve(&mut self, additional_capacity: usize) {
        self.set.reserve(additional_capacity)
    }

    /// Shrinks the capacity to the minimum allowed to hold the current number of elements.
    pub fn shrink_to_fit(&mut self) {
        self.set.shrink_to_fit()
    }

    /// Returns an over-approximation of the number of elements.
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// Is the set known to be empty?
    ///
    /// This could answer `false` for an empty set whose elements have
    /// expired but have yet to be collected.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// The proportion of buckets that are used.
    ///
    /// This is an over-approximation because of expired elements.
    pub fn load_factor(&self) -> f32 {
        self.set.load_factor()
    }

    /// Removes all actors from the group.
    pub fn clear(&mut self) {
        self.set.clear()
    }

    /// Returns true if the group contains the specified addr.
    pub fn contains(&self, key: &Addr<A>) -> bool {
        self.set.contains(&WeakTableAddrWrap(key.clone()))
    }

    /// Unconditionally inserts the addr, returning true if already present.
    pub fn insert(&mut self, key: Addr<A>) -> bool {
        self.set.insert(WeakTableAddrWrap(key))
    }

    /// Removes the addr, and returns true if it was present in this group.
    pub fn remove(&mut self, key: &Addr<A>) -> bool {
        self.set.remove(&WeakTableAddrWrap(key.clone()))
    }

    /// Removes all addrs not satisfying the given predicate.
    ///
    /// Also removes any expired actors.
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(Addr<A>) -> bool,
    {
        self.set
            .retain(|k: WeakTableAddrWrap<A>| f(k.deref().clone()))
    }

    /// Is self a subset of other?
    pub fn is_subset(&self, other: &GroupWeakAddr<A>) -> bool {
        self.set.is_subset(&other.set)
    }

    #[inline]
    /// Sends a message unconditionally, ignoring any potential errors.
    ///
    /// The message is always queued, even if the mailbox for the receiver is full.
    /// If the mailbox is closed, the message is silently dropped.
    pub fn do_send<M>(&self, msg: M)
    where
        M: Message + Send + Clone,
        M::Result: Send,
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
    {
        self.iter().for_each(|addr| {
            addr.do_send(msg.clone());
        });
    }

    /// Tries to send a message.
    ///
    /// This method fails if an actor's mailbox is full or closed. This
    /// method registers the current task in the receiver's queue.
    pub fn try_send<M>(&self, msg: M) -> Result<(), SendError<M>>
    where
        M: Message + Send + Clone + 'static,
        M::Result: Send,
        A: Handler<M>,
        A::Context: ToEnvelope<A, M>,
    {
        self.iter()
            .map(|addr| addr.try_send(msg.clone()))
            .fold(Ok(()), |acc, result| acc.and_then(|()| result))
    }
}
