//! Replacing a registry's contents with an authoritative set.

use std::sync::Arc;

use crate::logging;
use crate::resource::registry::merged_snapshot::MergedSnapshot;
use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{ApplyReport, RegistryResource, ResourceRegistry};
use crate::UnusableFirstLoad;

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
    /// # A first load is not a special call
    ///
    /// Applying primes the registry as a side effect, and nothing can un-prime
    /// it — so installing a set that leaves *nothing* to serve, on a registry
    /// that has never loaded, answers `/ready` with 200 over an empty snapshot
    /// while every request fails. That case is refused here, by this method, on
    /// every call.
    ///
    /// There used to be a second method for it, and a rule that callers must
    /// pick the right one. Two call sites had to agree about one fact and they
    /// drifted: the background refresh loop kept calling this method, so a prime
    /// that had been correctly refused was undone one refresh interval later —
    /// the same unusable payload installed as an empty snapshot, `/ready`
    /// flipping 503 → 200, and every request turning from a retryable
    /// `RuntimeUnavailable` into a terminal `UnknownTenant`.
    ///
    /// A call-site rule was never the right shape for it. "Has this registry
    /// ever loaded?" is a fact the registry knows about itself — the same fact
    /// [`Self::is_primed`] reports — so nothing is gained by asking a caller to
    /// remember it, and a caller that forgets cannot be caught by the compiler.
    /// The check now reads that fact off `guard`, the snapshot this merge is
    /// judged against, so the question and the merge cannot come apart.
    ///
    /// On a registry that already holds a snapshot this can never fail: a
    /// rejected resource falls back to the copy held, so there is always
    /// something left to serve.
    ///
    /// # Errors
    ///
    /// [`UnusableFirstLoad`] when the registry has never loaded, the set
    /// published resources, and none of them survived the merge. The registry is
    /// left untouched — and so, unprimed — rather than reporting ready over an
    /// empty snapshot.
    ///
    /// # Concurrency
    ///
    /// This is a read-modify-write, so it is serialised against the other
    /// mutators by the registry's write lock. Lookups are not affected.
    pub fn apply_all(&self, incoming: Vec<T>) -> Result<ApplyReport, UnusableFirstLoad> {
        // Held across the swap *and* the publish below: see `WriteLock` for
        // why both, rather than only the swap.
        let _write = self.writes.acquire();

        let guard = self.snapshot.load();
        let mut merged = MergedSnapshot::merge(guard.as_deref(), incoming);

        // Asked unconditionally. `refusal` takes no arguments precisely so that
        // this line cannot be made to ask a different question than the merge
        // answered, and so that there is nowhere left to put a branch that
        // skips it.
        if let Some(refusal) = merged.refusal() {
            return Err(refusal);
        }

        Ok(self.install(merged))
    }

    /// Swaps in a merged snapshot and announces what changed.
    ///
    /// The write lock is the caller's to hold: it is acquired before the merge,
    /// so the snapshot a merge was judged against — including whether there was
    /// one at all — is still the one being replaced here.
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
