//! Moving the generation when a transition ends, however it ends.

use std::sync::atomic::{AtomicU64, Ordering};

/// Moves the generation when dropped, however the transition ended.
///
/// A guard rather than a statement after the await, because a statement after
/// the await does not run when the transition panics or when its task is
/// dropped at shutdown — and a transition that died part-way has moved the
/// state just as surely as one that returned. Declared after the turn's guard
/// so it drops before it: the bump lands while the turn is still held.
pub(super) struct Bumped<'a>(pub(super) &'a AtomicU64);

impl Drop for Bumped<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
