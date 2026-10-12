//! What one reconciliation pass concluded.
//!
//! The hand-edit half of the type's surface — what a pass held, overwrote,
//! or attempted without confirmation (D01-19b) — is a second `impl` block in
//! `outcome/hand_edits.rs`, the house split for one type's two audiences.

mod hand_edits;

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

    /// Unsupported hand edits an explicit re-assertion overwrote, as
    /// confirmed by the provider.
    overwritten: Vec<HandEdit>,

    /// Unsupported hand edits on the client whose update failed: attempted,
    /// with an effect nobody can confirm.
    unconfirmed: Vec<HandEdit>,
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
            unconfirmed: Vec::new(),
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
            unconfirmed: Vec::new(),
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
            unconfirmed: Vec::new(),
        }
    }

    /// The pass failed after the provider confirmed `completed` actions.
    ///
    /// Still `Failed`: the plan did not finish. But the actions before the
    /// failing one took effect, and the count says so rather than claiming
    /// the pass changed nothing.
    #[must_use]
    pub fn failed_after(completed: usize, error: &ProviderError) -> Self {
        Self {
            actions: completed,
            ..Self::failed(error)
        }
    }

    /// Whether the pass succeeded.
    #[must_use]
    pub const fn status(&self) -> ReconciliationStatus {
        self.status
    }

    /// How many changes it made. Zero on a converged pass; on a failed pass,
    /// how many the provider confirmed before the failing one.
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
