//! Whether a publication pass is running now, and what the last one did.

use std::sync::Mutex;

use crate::publication::outcome::PassOutcome;
use crate::running_guard::RunningFlag;

/// Guards against two publication passes at once, and remembers the last
/// one.
///
/// [`crate::SweepState`]'s sibling, for the same reason the guard and the
/// record live together there: "is a pass running" and "what did the last
/// one do" are one console question, not two, and a caller asking only one
/// of them would be asking half of it.
#[derive(Debug, Default)]
pub struct PublicationState {
    /// Whether a pass is in progress.
    ///
    /// Guards re-entry rather than a real race: two passes running at once
    /// would both decide what to publish against the same held revisions,
    /// and the second's write is refused as stale by the publication target
    /// itself (ADR 0018 part 6). This exists to save the wasted read and
    /// write, not to close a race the port does not already close.
    ///
    /// A `RunningFlag`, not a bare `AtomicBool`, for the same reason
    /// `SweepState::running` is one: the swap that claims it and the guard
    /// that releases it are paired inside `RunningFlag::try_enter`, so
    /// neither can happen without the other.
    pub(super) running: RunningFlag,

    /// What the last completed pass found.
    last: Mutex<Option<LastPass>>,
}

impl PublicationState {
    /// A publication state with nothing recorded yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What the last completed pass found, if one has run.
    ///
    /// `None` means nothing has run yet, which is its own answer -- the
    /// same three-way distinction [`crate::SweepState::last_check`] draws
    /// between "never checked", "checked, nothing to do" and "checked,
    /// failed".
    #[must_use]
    pub fn last_pass(&self) -> Option<LastPass> {
        self.last.lock().ok().and_then(|last| last.clone())
    }

    /// Records how a pass ended.
    pub(super) fn record(&self, at_unix_seconds: u64, outcome: PassOutcome) {
        if let Ok(mut last) = self.last.lock() {
            *last = Some(LastPass {
                at_unix_seconds,
                outcome,
            });
        }
    }
}

/// What one publication pass did, and when it finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastPass {
    /// When it finished, as seconds since the Unix epoch.
    ///
    /// Not formatted here, for the same reason `LastCheck::at_unix_seconds`
    /// is not: recording a fact and rendering it for a human are different
    /// jobs, and only one of them belongs in a crate with no transport.
    pub at_unix_seconds: u64,

    /// What it found and did.
    pub outcome: PassOutcome,
}
