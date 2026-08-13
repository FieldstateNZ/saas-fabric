//! Replacing a registry's contents with an authoritative set.

use std::collections::HashMap;
use std::sync::Arc;

use crate::logging;
use crate::resource::registry::merge::{collect_removals, merge};
use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{ApplyReport, RegistryResource, ResourceRegistry};

impl<T: RegistryResource> ResourceRegistry<T> {
    /// Replaces the registry contents with an authoritative set.
    ///
    /// This is a **full sync**: a resource absent from `incoming` is treated as
    /// deprovisioned and removed. That is correct for a source publishing
    /// complete reconciled state, which is the model §6 describes.
    ///
    /// Per resource, an incoming copy whose revision is *older* than the one
    /// held is ignored and counted in [`ApplyReport::stale_ignored`]. Revisions
    /// only move forward (§20), so an older revision means a stale read, and
    /// applying it would resurrect retired state — potentially pointing a
    /// tenant back at a database a migration has just drained.
    ///
    /// Note the asymmetry: staleness is enforced per resource, but removal is
    /// taken at face value. A source publishing a *truncated* set will remove
    /// resources. That is the price of supporting deprovisioning at all, and it
    /// is why a load failure must never be turned into an empty set.
    ///
    /// # Validation
    ///
    /// Each incoming resource is checked by [`RegistryResource::validate`]
    /// before it may enter the snapshot. One that fails is dropped, and the
    /// previously-held copy — if there is one — is left in place. See that
    /// method for why a bad resource is skipped rather than failing the whole
    /// apply, and why the held copy is retained rather than removed.
    ///
    /// # Same revision, different payload
    ///
    /// An incoming copy at *exactly* the revision already held is never
    /// applied, even if its payload differs from what is held — see
    /// [`ApplyReport::divergent_payload`] for the full reasoning (item 50).
    /// The short version: the revision is the authority on whether a
    /// resource changed, so trusting a payload that disagrees with it would
    /// make the revision guard meaningless. The mismatch is counted and
    /// logged instead of being silently folded into "unchanged".
    ///
    /// # Concurrency
    ///
    /// This is a read-modify-write, so it is serialised against the other
    /// mutators by the registry's write lock. Lookups are not affected.
    pub fn apply_all(&self, incoming: Vec<T>) -> ApplyReport {
        // Held across the swap *and* the publish below: see `WriteLock` for
        // why both, rather than only the swap.
        let _write = self.writes.acquire();

        let guard = self.snapshot.load();
        let current = guard.as_ref();

        let mut report = ApplyReport::default();
        let mut next: HashMap<T::Key, Arc<T>> = HashMap::with_capacity(incoming.len());
        let mut events = Vec::new();

        for resource in incoming {
            let held = current.and_then(|snapshot| snapshot.get(resource.key()));

            merge(resource, held, &mut next, &mut report, &mut events);
        }

        if let Some(snapshot) = current {
            collect_removals(snapshot, &next, &mut report, &mut events);
        }

        let size = next.len();
        self.snapshot.store(Some(Arc::new(ResourceSnapshot::new(next))));

        // Published only after the swap, so a subscriber that reacts by looking
        // the resource up sees the new state rather than the old.
        self.publish(events);
        logging::snapshot_applied::<T>(size, &report);

        report
    }
}
