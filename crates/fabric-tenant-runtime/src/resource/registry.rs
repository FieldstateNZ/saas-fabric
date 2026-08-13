//! The registry: lock-free lookup over a swappable snapshot.

mod apply_all;
mod apply_one;
#[cfg(test)]
mod apply_tests;
#[cfg(test)]
mod change_tests;
#[cfg(test)]
mod concurrency_tests;
#[cfg(test)]
mod deletion_tests;
#[cfg(test)]
mod duplicate_key_tests;
#[cfg(test)]
mod lookup_tests;
mod merge;
#[cfg(test)]
mod stale_revision_tests;
#[cfg(test)]
mod test_resource;
#[cfg(test)]
mod validation_tests;
mod write_lock;
#[cfg(test)]
mod writer_concurrency_tests;

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use tokio::sync::broadcast;

use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{LookupError, RegistryResource, ResourceChange};
use write_lock::WriteLock;

/// How many change notifications are buffered for a slow subscriber before it
/// starts losing the oldest.
///
/// A lagging subscriber is told it lagged and can re-read current state, so
/// losing events is recoverable. Blocking the registry on a slow subscriber
/// would not be.
const CHANGE_CHANNEL_CAPACITY: usize = 256;

/// Holds the current set of one kind of reconciled resource.
///
/// # Priming
///
/// A fresh registry holds **no snapshot at all**, which is different from
/// holding an empty one. Until the first successful load every lookup returns
/// [`LookupError::Unavailable`] rather than [`LookupError::NotFound`] — see
/// [`LookupError`] for why that distinction is load-bearing.
///
/// # Writers
///
/// [`Self::apply_all`], [`Self::apply_one`] and [`Self::invalidate`] are each a
/// read-modify-write, so they are serialised against one another by an internal
/// write lock. Calling them concurrently is therefore safe — without that,
/// two writers would read the same starting snapshot and the second store would
/// discard the first, which also defeats the revision guard by letting a lower
/// revision land after a higher one.
///
/// Readers are unaffected and take no lock: [`Self::lookup`] remains an atomic
/// pointer load and a hash lookup no matter how many writers are active.
pub struct ResourceRegistry<T: RegistryResource> {
    snapshot: ArcSwapOption<ResourceSnapshot<T>>,
    changes: broadcast::Sender<ResourceChange<T::Key>>,
    writes: WriteLock,
}

impl<T: RegistryResource> Default for ResourceRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: RegistryResource> ResourceRegistry<T> {
    /// An unprimed registry. Resolves nothing until resources are applied.
    #[must_use]
    pub fn new() -> Self {
        let (changes, _) = broadcast::channel(CHANGE_CHANNEL_CAPACITY);

        Self {
            snapshot: ArcSwapOption::empty(),
            changes,
            writes: WriteLock::default(),
        }
    }

    /// Looks up a resource.
    ///
    /// The request-path operation: an atomic pointer load and a hash lookup. No
    /// I/O, no locks, no allocation beyond an `Arc` clone.
    ///
    /// # Errors
    ///
    /// [`LookupError::Unavailable`] if no snapshot has loaded yet, or
    /// [`LookupError::NotFound`] if the key is absent from the snapshot.
    pub fn lookup(&self, key: &T::Key) -> Result<Arc<T>, LookupError> {
        let guard = self.snapshot.load();

        let Some(snapshot) = guard.as_ref() else {
            return Err(LookupError::Unavailable);
        };

        snapshot.get(key).map(Arc::clone).ok_or(LookupError::NotFound)
    }

    /// Subscribes to resource transitions.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ResourceChange<T::Key>> {
        self.changes.subscribe()
    }

    /// Whether a snapshot has ever been loaded.
    ///
    /// Drives the readiness probe: a process that has not primed can serve
    /// nothing and should not receive traffic.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.snapshot.load().is_some()
    }

    /// How many resources are currently held. Zero when unprimed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshot.load().as_ref().map_or(0, |snapshot| snapshot.len())
    }

    /// Whether the registry holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Broadcasts change events, tolerating the common case of no subscribers.
    fn publish(&self, events: Vec<ResourceChange<T::Key>>) {
        for event in events {
            // `send` fails only when nobody is listening, which is normal at
            // startup and in tests. There is nothing to recover from.
            drop(self.changes.send(event));
        }
    }
}
