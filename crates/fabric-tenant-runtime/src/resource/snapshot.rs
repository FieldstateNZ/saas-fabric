//! One immutable view of every resource of a kind.

use std::collections::HashMap;
use std::sync::Arc;

use crate::resource::RegistryResource;

/// An immutable map of every resource the registry currently holds.
///
/// # Why a whole-snapshot swap rather than a locked map
///
/// Resolution happens on every request and must not contend. An
/// `RwLock<HashMap<..>>` would serialise readers against any writer, so a
/// refresh — which touches every entry — would stall the entire request path
/// for its duration.
///
/// Instead the registry builds a fresh snapshot to one side and swaps a pointer
/// to it. Readers take an atomic load and never block, refreshes never stall a
/// request, and an in-flight request keeps the snapshot it started with rather
/// than seeing a half-applied update.
///
/// The cost is a full rebuild per refresh — a copy of a few thousand small
/// `Arc`s, cheaper than one database round trip, and the request path is where
/// the budget matters.
#[derive(Debug)]
pub(crate) struct ResourceSnapshot<T: RegistryResource> {
    entries: HashMap<T::Key, Arc<T>>,
    facts: T::SetFacts,
}

impl<T: RegistryResource> ResourceSnapshot<T> {
    /// Builds a snapshot, deriving its whole-set facts from the same map.
    ///
    /// The derivation is here rather than at the three call sites so the two
    /// cannot come apart: a snapshot's facts always describe the entries it
    /// actually holds, whether it was built by a full sync, a single apply, or
    /// an invalidation.
    pub(crate) fn new(entries: HashMap<T::Key, Arc<T>>) -> Self {
        let facts = T::derive_set_facts(&entries);

        Self { entries, facts }
    }

    /// The whole-set facts derived when this snapshot was built.
    pub(crate) const fn facts(&self) -> &T::SetFacts {
        &self.facts
    }

    /// Looks up a resource.
    pub(crate) fn get(&self, key: &T::Key) -> Option<&Arc<T>> {
        self.entries.get(key)
    }

    /// Borrows the map, for building the next snapshot from this one.
    pub(crate) const fn entries(&self) -> &HashMap<T::Key, Arc<T>> {
        &self.entries
    }

    /// How many resources this snapshot holds.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}
