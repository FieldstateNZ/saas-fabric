//! Deciding the fate of one incoming resource during a full sync.

use std::sync::Arc;

use crate::logging;
use crate::resource::registry::merged_snapshot::MergedSnapshot;
use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{RegistryResource, ResourceChange};

impl<T: RegistryResource> MergedSnapshot<T> {
    /// Judges one incoming resource against `held`, the copy in the snapshot
    /// being replaced.
    ///
    /// # Why validity is judged before duplication
    ///
    /// Both checks refuse an entry, but they refuse it for opposite reasons, so
    /// the order is load-bearing rather than stylistic.
    ///
    /// Validity is a property of the entry alone — no snapshot involved. Running
    /// it first, unconditionally, is the version of "nothing invalid ever
    /// reaches the snapshot" a reviewer can confirm by reading one line, and it
    /// cannot be undone by someone adding a branch below.
    ///
    /// Duplication is about the *comparison*: a key may be compared against the
    /// outgoing snapshot only once per call, because two entries would both
    /// compare against that same outgoing snapshot while only one could win the
    /// map — and the loser's event would then describe a snapshot that never
    /// existed. A subscriber acting on it (§19 — drop the state attached to the
    /// old revision, re-read the resource) would look up a revision the registry
    /// does not hold.
    ///
    /// An invalid entry never reaches that comparison, so it does not consume
    /// the key. Checking duplication first said otherwise, and that is what
    /// made `[invalid a@1, valid a@2]` install nothing at all: the first entry
    /// claimed the key on its way to being dropped, and the entry that could
    /// actually be served was refused as a duplicate of it. So the rule is not
    /// "the first entry for a key decides it" but **the first entry that can be
    /// compared decides it** — still position, never revision, because the
    /// revision is exactly the field a duplicated key calls into question.
    pub(super) fn accept(&mut self, incoming: T, held: Option<&Arc<T>>) {
        if let Err(error) = incoming.validate() {
            self.report.invalid_rejected += 1;
            logging::invalid_resource_rejected::<T>(incoming.key(), &error);

            if self.first_rejection.is_none() {
                self.first_rejection = Some(format!("{} {}: {error}", T::KIND, incoming.key()));
            }

            // Retained, not removed: an unusable payload is a reconciler bug,
            // not a deprovisioning. See `RegistryResource::validate`. Skipped
            // when the key is already decided, so this cannot overwrite the
            // entry that won it.
            if let Some(held) = held {
                if !self.decided.contains(incoming.key()) {
                    self.next.insert(incoming.key().clone(), Arc::clone(held));
                }
            }

            return;
        }

        if !self.decided.insert(incoming.key().clone()) {
            self.report.duplicate_rejected += 1;
            logging::duplicate_key_rejected::<T>(incoming.key(), incoming.revision());
            return;
        }

        self.against_held(incoming, held);
    }

    /// Applies the revision guard to a validated, first-seen resource.
    fn against_held(&mut self, incoming: T, held: Option<&Arc<T>>) {
        match held {
            None => {
                self.report.added += 1;
                self.events
                    .push(ResourceChange::added(incoming.key().clone(), incoming.revision()));
                self.next.insert(incoming.key().clone(), Arc::new(incoming));
            }
            Some(held) if incoming.revision() > held.revision() => {
                self.report.updated += 1;
                self.events.push(ResourceChange::updated(
                    incoming.key().clone(),
                    held.revision(),
                    incoming.revision(),
                ));
                self.next.insert(incoming.key().clone(), Arc::new(incoming));
            }
            Some(held) if incoming.revision() < held.revision() => {
                self.report.stale_ignored += 1;
                logging::stale_resource_ignored::<T>(incoming.key(), incoming.revision(), held.revision());
                self.next.insert(incoming.key().clone(), Arc::clone(held));
            }
            // Same revision, same payload: the ordinary no-op.
            Some(held) if incoming == **held => {
                self.report.unchanged += 1;
                self.next.insert(incoming.key().clone(), Arc::clone(held));
            }
            // Same revision, different payload: item 50. Never applied — the
            // revision is the authority — but counted and logged so a
            // reconciler bug (a real change that forgot to bump the revision)
            // cannot vanish without a trace.
            Some(held) => {
                self.report.divergent_payload += 1;
                logging::divergent_payload_at_same_revision::<T>(incoming.key(), incoming.revision());
                self.next.insert(incoming.key().clone(), Arc::clone(held));
            }
        }
    }

    /// Records everything the incoming set dropped.
    ///
    /// A resource rejected as invalid is *not* dropped: [`Self::accept`] puts
    /// the held copy back first, so it does not appear here as a removal and no
    /// [`ChangeKind::Removed`](crate::resource::ChangeKind::Removed) event fires
    /// for it.
    pub(super) fn collect_removals(&mut self, current: &ResourceSnapshot<T>) {
        for (key, held) in current.entries() {
            if !self.next.contains_key(key) {
                self.report.removed += 1;
                self.events
                    .push(ResourceChange::removed(key.clone(), held.revision()));
            }
        }
    }
}
