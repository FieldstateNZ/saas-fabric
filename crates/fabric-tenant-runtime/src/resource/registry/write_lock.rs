//! Serialising the registry's writers without touching the read path.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// The lock every registry mutator takes before its read-modify-write.
///
/// # The bug this closes
///
/// `apply_all`, `apply_one` and `invalidate` each load the current snapshot,
/// build the next one from it, and store that back. Run two of those
/// concurrently and both read the same starting snapshot, so whichever stores
/// second overwrites the other's work wholesale — no error, no log line.
///
/// The lost update is the mild half. The serious half is that it defeats the
/// revision guard: a writer that read revision 1 and concluded "my revision 3
/// is newer" can store *after* a writer that installed revision 5, leaving the
/// registry holding 3. Revisions only move forward (§20) is the invariant that
/// stops a stale read pointing a tenant back at a database a migration has
/// already drained, and a guard evaluated against a snapshot that is stale by
/// the time it is acted on does not enforce it.
///
/// Today there is one [`ResourceRefresher`](crate::ResourceRefresher) per
/// registry, so nothing in this crate reaches the race. That is a property of
/// the current composition root, not of the type: all three mutators are `pub`,
/// and an undocumented "only call these from one thread" rule is the kind of
/// constraint that holds right up until somebody adds a second caller.
///
/// # Why a lock rather than `compare_and_swap`
///
/// [`ArcSwapOption`](arc_swap::ArcSwapOption) offers a compare-and-swap, so a
/// retry loop was the alternative. Two things decided it:
///
/// - A losing `apply_all` would have to throw away a freshly-built snapshot of
///   every resource, plus its change events, and redo the lot. Precisely under
///   the contention that would justify a retry loop, retrying is the expensive
///   path.
/// - Change events have to reach subscribers in the order the snapshots were
///   installed. Holding this lock across the store *and* the broadcast gives
///   that for free. A CAS loop publishes outside any mutual exclusion, so two
///   writers can install A then B but publish B then A — telling a subscriber
///   to drop attached state (§19) that is, by then, current again.
///
/// Readers pay nothing under either scheme, which is the point worth being
/// explicit about: [`lookup`](super::ResourceRegistry::lookup) never touches
/// this lock. It stays a single atomic load and a hash lookup, which is the
/// whole reason the snapshot lives in an `ArcSwapOption` rather than behind an
/// `RwLock`.
#[derive(Debug, Default)]
pub(super) struct WriteLock(Mutex<()>);

impl WriteLock {
    /// Blocks until this writer may proceed.
    ///
    /// Poisoning is recovered from rather than propagated. What this guards is
    /// `()`: a writer that panicked left no half-built state behind, because
    /// the snapshot it was assembling was a local that went with its stack and
    /// the published one is only ever swapped in complete. Propagating the
    /// poison would turn one panicking refresh into a registry that can never
    /// be written again — trading a recoverable blip for a permanent outage.
    pub(super) fn acquire(&self) -> MutexGuard<'_, ()> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
