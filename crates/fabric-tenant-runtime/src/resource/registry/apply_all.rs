//! Replacing a registry's contents with an authoritative set.

use std::sync::Arc;

use crate::logging;
use crate::resource::registry::merged_snapshot::MergedSnapshot;
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
    /// Each incoming resource is checked by [`RegistryResource::validate`]
    /// before it may enter the snapshot; one that fails is dropped and the
    /// previously-held copy, if any, is left in place. A copy arriving at
    /// *exactly* the revision already held is never applied even when its
    /// payload differs — see [`ApplyReport::divergent_payload`]. A key
    /// appearing twice in one set is decided by its first comparable entry and
    /// the rest are refused — see
    /// `MergedSnapshot::accept` for
    /// the ordering between those two rules and why it matters.
    ///
    /// # This installs unconditionally
    ///
    /// Applying primes the registry as a side effect, and nothing can un-prime
    /// it. On a registry that has never loaded, use
    /// `apply_first_load` instead: it runs the identical merge and
    /// installs the result only if there is something to serve. Calling this
    /// method for a first load can leave a registry primed and empty, which
    /// answers `/ready` with 200 while failing every request.
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
        let merged = MergedSnapshot::merge(guard.as_deref(), incoming);

        self.install(merged)
    }

    /// Applies the **first** set a registry ever sees, refusing to install one
    /// that would leave nothing to serve.
    ///
    /// The merge is the same one [`Self::apply_all`] runs — the same code
    /// decides what to install and whether installing it is safe, which is the
    /// invariant [`MergedSnapshot`] exists to hold. All this adds is *when* the
    /// swap happens: after the verdicts are in, not before.
    ///
    /// An empty publication still primes. A deployment with no tenants
    /// onboarded yet must be able to start, and installing nothing is only a
    /// failure when something was offered to install. A partial rejection also
    /// primes, for the reason set out on [`MergedSnapshot::refusal`].
    ///
    /// # Errors
    ///
    /// The first rejection, named, when the set published resources and none of
    /// them survived. The registry is left untouched — and so, on a fresh
    /// registry, unprimed — rather than reporting ready over an empty snapshot.
    pub(crate) fn apply_first_load(&self, incoming: Vec<T>) -> Result<ApplyReport, String> {
        let _write = self.writes.acquire();

        let published = incoming.len();
        let guard = self.snapshot.load();
        let mut merged = MergedSnapshot::merge(guard.as_deref(), incoming);

        if let Some(reason) = merged.refusal(published) {
            return Err(reason);
        }

        Ok(self.install(merged))
    }

    /// Swaps in a merged snapshot and announces what changed.
    ///
    /// The write lock is the caller's to hold: both callers acquire it before
    /// the merge, so the snapshot a merge was judged against is still the one
    /// being replaced here.
    fn install(&self, merged: MergedSnapshot<T>) -> ApplyReport {
        let size = merged.next.len();
        self.snapshot
            .store(Some(Arc::new(ResourceSnapshot::new(merged.next))));

        // Published only after the swap, so a subscriber that reacts by looking
        // the resource up sees the new state rather than the old.
        self.publish(merged.events);
        logging::snapshot_applied::<T>(size, &merged.report);

        merged.report
    }
}
