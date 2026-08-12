//! Replacing a registry's contents with an authoritative set.

use std::collections::HashMap;
use std::sync::Arc;

use crate::logging;
use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{ApplyReport, RegistryResource, ResourceChange, ResourceRegistry};

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
    /// # Same revision, different payload
    ///
    /// An incoming copy at *exactly* the revision already held is never
    /// applied, even if its payload differs from what is held — see
    /// [`ApplyReport::divergent_payload`] for the full reasoning (item 50).
    /// The short version: the revision is the authority on whether a
    /// resource changed, so trusting a payload that disagrees with it would
    /// make the revision guard meaningless. The mismatch is counted and
    /// logged instead of being silently folded into "unchanged".
    pub fn apply_all(&self, incoming: Vec<T>) -> ApplyReport {
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

/// Decides what happens to one incoming resource.
fn merge<T: RegistryResource>(
    incoming: T,
    held: Option<&Arc<T>>,
    next: &mut HashMap<T::Key, Arc<T>>,
    report: &mut ApplyReport,
    events: &mut Vec<ResourceChange<T::Key>>,
) {
    match held {
        None => {
            report.added += 1;
            events.push(ResourceChange::added(incoming.key().clone(), incoming.revision()));
            next.insert(incoming.key().clone(), Arc::new(incoming));
        }
        Some(held) if incoming.revision() > held.revision() => {
            report.updated += 1;
            events.push(ResourceChange::updated(
                incoming.key().clone(),
                held.revision(),
                incoming.revision(),
            ));
            next.insert(incoming.key().clone(), Arc::new(incoming));
        }
        Some(held) if incoming.revision() < held.revision() => {
            report.stale_ignored += 1;
            logging::stale_resource_ignored::<T>(incoming.key(), incoming.revision(), held.revision());
            next.insert(incoming.key().clone(), Arc::clone(held));
        }
        // Same revision, same payload: the ordinary no-op.
        Some(held) if incoming == **held => {
            report.unchanged += 1;
            next.insert(incoming.key().clone(), Arc::clone(held));
        }
        // Same revision, different payload: item 50. Never applied — the
        // revision is the authority — but counted and logged so a
        // reconciler bug (a real change that forgot to bump the revision)
        // cannot vanish without a trace.
        Some(held) => {
            report.divergent_payload += 1;
            logging::divergent_payload_at_same_revision::<T>(incoming.key(), incoming.revision());
            next.insert(incoming.key().clone(), Arc::clone(held));
        }
    }
}

/// Records everything the incoming set dropped.
fn collect_removals<T: RegistryResource>(
    current: &ResourceSnapshot<T>,
    next: &HashMap<T::Key, Arc<T>>,
    report: &mut ApplyReport,
    events: &mut Vec<ResourceChange<T::Key>>,
) {
    for (key, held) in current.entries() {
        if !next.contains_key(key) {
            report.removed += 1;
            events.push(ResourceChange::removed(key.clone(), held.revision()));
        }
    }
}
