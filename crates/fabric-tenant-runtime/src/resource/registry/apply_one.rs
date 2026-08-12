//! Incremental updates to a single resource.

use std::sync::Arc;

use crate::logging;
use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{RegistryResource, ResourceChange, ResourceRegistry};

impl<T: RegistryResource> ResourceRegistry<T> {
    /// Applies a single resource without touching the rest.
    ///
    /// Used for incremental updates — a reconciler notifying the runtime that
    /// one thing changed rather than republishing everything.
    ///
    /// Returns `false` if the incoming revision is not newer than what is held,
    /// in which case nothing changes. That includes the revision matching
    /// exactly: even if the payload differs, it is never applied here — the
    /// revision remains the sole authority for "did this change" (item 50,
    /// see [`ApplyReport::divergent_payload`](crate::resource::ApplyReport::divergent_payload)
    /// on the full reasoning that `apply_all` shares). A same-revision
    /// mismatch is logged at warn level rather than silently accepted or
    /// silently dropped.
    ///
    /// On an unprimed registry this primes it with a single entry. That is
    /// intentional for tests and incremental-only deployments, but a production
    /// start should `apply_all` first so that lookups do not report everything
    /// else as missing.
    pub fn apply_one(&self, resource: T) -> bool {
        let guard = self.snapshot.load();

        let mut next = guard
            .as_ref()
            .map(|snapshot| snapshot.entries().clone())
            .unwrap_or_default();

        let event = match next.get(resource.key()) {
            Some(held) if resource.revision() < held.revision() => {
                logging::stale_resource_ignored::<T>(resource.key(), resource.revision(), held.revision());
                return false;
            }
            Some(held) if resource.revision() == held.revision() => {
                if resource != **held {
                    logging::divergent_payload_at_same_revision::<T>(resource.key(), resource.revision());
                }
                return false;
            }
            Some(held) => {
                ResourceChange::updated(resource.key().clone(), held.revision(), resource.revision())
            }
            None => ResourceChange::added(resource.key().clone(), resource.revision()),
        };

        next.insert(resource.key().clone(), Arc::new(resource));
        self.snapshot.store(Some(Arc::new(ResourceSnapshot::new(next))));
        self.publish(vec![event]);

        true
    }

    /// Drops one resource.
    ///
    /// Returns `false` if it was not held. Afterwards, looking it up fails
    /// closed until the next refresh restores it — the intended behaviour for
    /// something deprovisioned, and an acceptable momentary outage for
    /// something invalidated by mistake.
    pub fn invalidate(&self, key: &T::Key) -> bool {
        let guard = self.snapshot.load();

        let Some(snapshot) = guard.as_ref() else {
            return false;
        };

        let Some(held) = snapshot.get(key) else {
            return false;
        };

        let revision = held.revision();
        let mut next = snapshot.entries().clone();
        next.remove(key);

        self.snapshot.store(Some(Arc::new(ResourceSnapshot::new(next))));
        self.publish(vec![ResourceChange::removed(key.clone(), revision)]);

        true
    }
}
