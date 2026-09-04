//! What one reconciliation did.

use crate::{ComponentStatus, Version};

/// What one reconciliation did.
///
/// Carries where the component started as well as where it ended, because
/// "advanced" and "was already there" produce the same status and are not the
/// same event. A sweep reports one and stays quiet about the other.
///
/// # Beside `ComponentStatus` rather than inside its file
///
/// A status is what the *console* is told about a component, and this is what
/// a *sweep* did to one. They share a type and answer to different callers,
/// which is enough to be two concepts — and keeping them apart is what left
/// the status file room to explain itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciliation {
    /// The version desired before this ran.
    pub was: Version,

    /// The situation afterwards.
    pub status: ComponentStatus,
}

impl Reconciliation {
    /// Whether desired state moved.
    #[must_use]
    pub fn advanced(&self) -> bool {
        self.was != self.status.desired
    }
}
