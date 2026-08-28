//! What one reconciliation pass concluded.

use crate::provider::ProviderError;
use crate::status::ReconciliationStatus;

/// The result of one pass over one client.
///
/// The status here is only ever [`Applied`](ReconciliationStatus::Applied) or
/// [`Failed`](ReconciliationStatus::Failed) — a pass has, by definition,
/// happened, so it cannot be `Pending`, and whether a correction counts as
/// *drift* depends on what was recorded last time, which this type has no way
/// to know. That judgement belongs to the status store, and keeping it out of
/// here is what stops two places deciding it differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationOutcome {
    /// Whether the pass succeeded.
    status: ReconciliationStatus,

    /// How many changes it made.
    actions: usize,

    /// Why it failed, if it did. Sanitised by the adapter that produced it.
    detail: Option<String>,
}

impl ReconciliationOutcome {
    /// The provider already matched: nothing was changed.
    ///
    #[must_use]
    pub const fn converged() -> Self {
        Self {
            status: ReconciliationStatus::Applied,
            actions: 0,
            detail: None,
        }
    }

    /// The provider was changed, successfully.
    #[must_use]
    pub const fn applied(actions: usize) -> Self {
        Self {
            status: ReconciliationStatus::Applied,
            actions,
            detail: None,
        }
    }

    /// The pass failed.
    ///
    /// The detail is the provider error's own message, which
    /// [`ProviderError`] documents as safe to record: it carries no upstream
    /// response body and no credential.
    #[must_use]
    pub fn failed(error: &ProviderError) -> Self {
        Self {
            status: ReconciliationStatus::Failed,
            actions: 0,
            detail: Some(error.to_string()),
        }
    }

    /// Whether the pass succeeded.
    #[must_use]
    pub const fn status(&self) -> ReconciliationStatus {
        self.status
    }

    /// How many changes it made. Zero on a converged or failed pass.
    #[must_use]
    pub const fn actions(&self) -> usize {
        self.actions
    }

    /// Why it failed, if it did.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Whether the provider was already in the desired state.
    #[must_use]
    pub const fn changed_nothing(&self) -> bool {
        self.actions == 0
    }
}
