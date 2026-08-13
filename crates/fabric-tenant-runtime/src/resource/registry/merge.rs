//! Deciding the fate of one incoming resource during a full sync.

use std::collections::HashMap;
use std::sync::Arc;

use crate::logging;
use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{ApplyReport, RegistryResource, ResourceChange};

/// Decides what happens to one incoming resource.
///
/// The checks run in a fixed order — validity, then the revision guard — and
/// validity comes first deliberately. Checking every incoming resource
/// unconditionally is the version of "nothing invalid ever reaches the
/// snapshot" that a reviewer can confirm by reading one line, and it cannot be
/// undone by someone adding a match arm below.
pub(super) fn merge<T: RegistryResource>(
    incoming: T,
    held: Option<&Arc<T>>,
    next: &mut HashMap<T::Key, Arc<T>>,
    report: &mut ApplyReport,
    events: &mut Vec<ResourceChange<T::Key>>,
) {
    if let Err(error) = incoming.validate() {
        report.invalid_rejected += 1;
        logging::invalid_resource_rejected::<T>(incoming.key(), &error);

        // Retained, not removed: an unusable payload is a reconciler bug, not
        // a deprovisioning. See `RegistryResource::validate`.
        if let Some(held) = held {
            next.insert(incoming.key().clone(), Arc::clone(held));
        }

        return;
    }

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
///
/// A resource rejected as invalid is *not* dropped: `merge` puts the held copy
/// back into `next` first, so it does not appear here as a removal and no
/// [`ChangeKind::Removed`](crate::resource::ChangeKind::Removed) event fires
/// for it.
pub(super) fn collect_removals<T: RegistryResource>(
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
