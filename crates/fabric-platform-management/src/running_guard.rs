//! Pairing the swap that claims a running flag with the guard that
//! releases it, so the two cannot be taken apart.
//!
//! [`RunningFlag::try_enter`] is the only way to win the flag, and the
//! only way to clear it is to drop the [`RunningGuard`] it hands back --
//! nothing outside this module can reach the inner `AtomicBool` to swap or
//! store it directly.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether a guarded operation -- a sweep, a publication pass -- is in
/// progress.
///
/// A newtype around `AtomicBool` rather than the bool itself: the only
/// operation this exposes is [`try_enter`](Self::try_enter), which performs
/// the swap and returns the guard in the same call, so a caller can never
/// win the swap without receiving the guard that promises to release it,
/// or release it without having won the swap first.
#[derive(Debug, Default)]
pub(crate) struct RunningFlag(AtomicBool);

impl RunningFlag {
    /// Claims the flag if nothing else currently holds it.
    ///
    /// `None` means another caller already won; this call's own job from
    /// there is to answer with whatever "already running" means to it and
    /// touch nothing else. `Some` hands back the one and only way to
    /// release the flag again.
    #[must_use]
    pub(crate) fn try_enter(&self) -> Option<RunningGuard<'_>> {
        if self.0.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(RunningGuard { flag: &self.0 })
        }
    }
}

/// Clears its flag when dropped -- on a normal return, an early return,
/// cancellation (the caller drops the future holding this guard at an
/// await point), or a panic unwinding through it.
///
/// Only [`RunningFlag::try_enter`] constructs one, and only after winning
/// the swap: this type's field is private to this module, so holding a
/// `RunningGuard` is itself the proof that the swap was won, not merely a
/// convention callers are expected to follow.
///
/// A plain `store(false, ...)` placed after the guarded work's own `.await`
/// is sequential code, and sequential code after an `.await` is exactly
/// what a panic unwinding out of that work skips, and exactly what never
/// runs when the caller drops the future at that suspension point -- an
/// operator's disconnect, or a request timeout, cancels the task holding it
/// the same way. `Drop` runs on both of those paths as well as the
/// ordinary one, which is the only way to make "always released" true
/// rather than merely usual.
pub(crate) struct RunningGuard<'a> {
    flag: &'a AtomicBool,
}

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}
