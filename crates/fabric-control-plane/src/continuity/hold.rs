//! Why a client's lifecycle is not confirmed by the continuity store, as an
//! operator and an API response see it.

use super::ContinuityRefusal;

/// Why an identity side effect, or a lifecycle write, is held.
///
/// Every variant is decided before anything is written: nothing is written,
/// reserved, audited, converged or published after one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ContinuityHold {
    /// This deployment has no protected continuity store. No retry changes
    /// it until the store exists.
    #[error("held: this deployment has no protected continuity store to confirm the client's lifecycle")]
    NoStore,
    /// The store could not be reached.
    #[error("held: the continuity store could not be reached")]
    Unavailable,
    /// The store holds no history for this client.
    #[error("held: the continuity store holds no accepted lifecycle for this client")]
    Unenrolled,
    /// The document's lifecycle is not the one the store accepted: a Git
    /// revert, an erased or forged history, or a write the store refused.
    #[error("held: the client's recorded lifecycle does not match the continuity store's")]
    Mismatch,
    /// A lifecycle transition is reserved and not confirmed.
    #[error("held: a lifecycle transition for this client is reserved and not confirmed")]
    Pending,
    /// The client id names an identity the store already knows.
    #[error("held: the continuity store already knows this client, so it is not a new identity")]
    Resurrection,
}

impl ContinuityHold {
    /// The hold a store's refusal amounts to.
    #[must_use]
    pub const fn from_refusal(refusal: &ContinuityRefusal) -> Self {
        match refusal {
            ContinuityRefusal::Unenrolled => Self::Unenrolled,
            ContinuityRefusal::Mismatch | ContinuityRefusal::StaleCompletion => Self::Mismatch,
            ContinuityRefusal::Pending => Self::Pending,
            ContinuityRefusal::Resurrection => Self::Resurrection,
            ContinuityRefusal::Unavailable { .. } => Self::Unavailable,
        }
    }
}
