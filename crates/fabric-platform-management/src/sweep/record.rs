//! What the last sweep found, for a console line.

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use crate::{PlatformError, SafeDiagnostic, Sweep, Swept};

/// What a sweep found, last time one ran.
///
/// # Why the console needs this at all
///
/// When a published version does not appear, there are three explanations and
/// they lead three different places: nothing has checked yet, something
/// checked and there was nothing to do, or something checked and failed.
/// Without this, all three look identical — which is the situation that
/// started this work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastCheck {
    /// When it finished, as seconds since the Unix epoch.
    ///
    /// Not formatted here. Recording a fact and rendering it for a human are
    /// different jobs, and only one of them belongs in a crate with no
    /// transport.
    pub at_unix_seconds: u64,

    /// What it found.
    pub outcome: CheckOutcome,
}

/// How a sweep ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// Every component was looked at. Some may have advanced; none failed.
    Succeeded,

    /// At least one component could not be reconciled, or the environment
    /// itself could not be read.
    Failed {
        /// What went wrong, ready to put beside a timestamp.
        ///
        /// A [`SafeDiagnostic`] rather than a `String`, so this cannot come to
        /// hold an upstream error somebody formatted with `Debug`. The console
        /// is the one place a leaked credential would be hardest to notice and
        /// easiest to forward.
        detail: SafeDiagnostic,
    },
}

/// Guards against two sweeps at once, and remembers the last one.
///
/// The guard and the record live together because they are the same
/// concern — what is happening, and what happened last — and a console asking
/// one without the other would be asking half a question.
#[derive(Debug, Default)]
pub struct SweepState {
    /// Whether a sweep is in progress.
    pub(super) running: AtomicBool,

    /// What the last completed sweep found.
    last: Mutex<Option<LastCheck>>,
}

impl SweepState {
    /// What the last completed sweep found, if one has.
    ///
    /// `None` means nothing has checked yet, which is its own answer.
    #[must_use]
    pub fn last_check(&self) -> Option<LastCheck> {
        self.last.lock().ok().and_then(|last| last.clone())
    }

    /// Records how a sweep ended.
    pub(super) fn record(&self, at_unix_seconds: u64, outcome: CheckOutcome) {
        if let Ok(mut last) = self.last.lock() {
            *last = Some(LastCheck {
                at_unix_seconds,
                outcome,
            });
        }
    }
}

/// How a sweep's result reads on one line.
pub(super) fn outcome_of(swept: &Result<Sweep, PlatformError>) -> CheckOutcome {
    let sweep = match swept {
        Ok(sweep) => sweep,
        Err(error) => {
            return CheckOutcome::Failed {
                detail: SafeDiagnostic::sanitise(&error.to_string()),
            }
        }
    };

    // The first failure, named. A console line has room for one, and an
    // operator who fixes it will see the next on the following sweep.
    for (component, swept) in &sweep.components {
        if let Swept::Failed(error) = swept {
            return CheckOutcome::Failed {
                detail: SafeDiagnostic::sanitise(&format!("{component}: {error}")),
            };
        }
    }

    CheckOutcome::Succeeded
}
