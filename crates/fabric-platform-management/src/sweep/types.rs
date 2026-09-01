//! What a sweep did, and whether it ran.

use crate::{PlatformError, Version};

/// What one component did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Swept {
    /// Desired state moved.
    Advanced {
        /// What it was on.
        from: Version,

        /// What it is on now.
        to: Version,
    },

    /// Nothing to do, or nothing permitted. The status says which.
    Unchanged,

    /// This component could not be reconciled.
    ///
    /// Carried rather than returned, because one component failing is not a
    /// reason to stop looking after the others — and a sweep that aborted on
    /// the first failure would leave every component after it in the list
    /// permanently unreconciled, in an order nobody chose.
    Failed(PlatformError),
}

/// What a sweep did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sweep {
    /// One entry per component, in the order they were read.
    pub components: Vec<(String, Swept)>,
}

impl Sweep {
    /// Whether anything failed.
    #[must_use]
    pub fn had_failures(&self) -> bool {
        self.components
            .iter()
            .any(|(_, swept)| matches!(swept, Swept::Failed(_)))
    }
}

/// Whether a sweep ran at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepResult {
    /// It ran, and this is what it did.
    Ran(Sweep),

    /// Nothing is connected, so there was nothing to sweep.
    ///
    /// Not a failure and not recorded. An operator has not connected a
    /// platform repository yet, and a "last check failed" against an
    /// integration that does not exist would send them looking for a fault
    /// instead of a connection they have not made.
    NotConnected,

    /// Another sweep was still going, so this one did nothing.
    ///
    /// Skipped rather than queued. A sweep that overruns its interval means
    /// registries or Git are slow, and the answer to that is not to start a
    /// second one behind it.
    AlreadyRunning,
}
