//! What one reconciliation pass concluded.
//!
//! In the 121–150 line band: one type, its constructors and its accessors.
//! Each pair of fields — what the pass did, and which hand edits it held or
//! overwrote — is read by the status store and the audit record together.

use crate::provider::{HandEdit, ProviderError};
use crate::status::ReconciliationStatus;

/// The result of one pass over one client.
///
/// The status here is only ever [`Applied`](ReconciliationStatus::Applied) or
/// [`Failed`](ReconciliationStatus::Failed) — a pass has, by definition,
/// happened, so it cannot be `Pending`, and whether a correction counts as
/// *drift* depends on what was recorded last time, which this type has no way
/// to know. That judgement belongs to the status store, and keeping it out of
/// here is what stops two places deciding it differently. The same goes for
/// `handEdited`: the outcome carries the hand edits it held, and the store
/// decides what that is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationOutcome {
    /// Whether the pass succeeded.
    status: ReconciliationStatus,

    /// How many changes it made.
    actions: usize,

    /// Why it failed, if it did. Sanitised by the adapter that produced it.
    detail: Option<String>,

    /// Unsupported hand edits the pass found and left in place (D01-19b).
    hand_edits: Vec<HandEdit>,

    /// Unsupported hand edits an explicit re-assertion overwrote.
    overwritten: Vec<HandEdit>,
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
            hand_edits: Vec::new(),
            overwritten: Vec::new(),
        }
    }

    /// The provider was changed, successfully.
    #[must_use]
    pub const fn applied(actions: usize) -> Self {
        Self {
            status: ReconciliationStatus::Applied,
            actions,
            detail: None,
            hand_edits: Vec::new(),
            overwritten: Vec::new(),
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
            hand_edits: Vec::new(),
            overwritten: Vec::new(),
        }
    }

    /// The same outcome, having left these hand edits in place.
    #[must_use]
    pub fn holding(mut self, hand_edits: Vec<HandEdit>) -> Self {
        self.hand_edits = hand_edits;
        self
    }

    /// The same outcome, having overwritten these hand edits at an
    /// operator's explicit request.
    #[must_use]
    pub fn overwriting(mut self, overwritten: Vec<HandEdit>) -> Self {
        self.overwritten = overwritten;
        self
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

    /// The unsupported hand edits this pass left in place.
    #[must_use]
    pub fn hand_edits(&self) -> &[HandEdit] {
        &self.hand_edits
    }

    /// The unsupported hand edits this pass overwrote, which only a
    /// re-assertion does.
    #[must_use]
    pub fn overwritten(&self) -> &[HandEdit] {
        &self.overwritten
    }

    /// Whether the provider was already in the desired state.
    #[must_use]
    pub const fn changed_nothing(&self) -> bool {
        self.actions == 0
    }
}
