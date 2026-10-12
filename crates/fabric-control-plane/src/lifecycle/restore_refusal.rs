//! Why a restore was refused before anything was written, beyond the
//! lifecycle's own refusals (D01-14 restore).

/// A restore refused by the continuity store or the retained-placement check.
///
/// Every variant is decided before the document is written: nothing is
/// written, reserved, audited or reconciled after one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RestoreRefusal {
    /// This deployment has no protected continuity store, so a restoration
    /// could not be told from a Git edit. Held, not failed: no retry changes
    /// it until the store exists (#108 comment 6119161317).
    #[error(
        "restoring a tombstone is held: this deployment has no protected continuity store to record it in, \
         so nothing was written"
    )]
    ContinuityAbsent,

    /// The store could not be reached.
    #[error("the continuity store could not be reached, so nothing was written")]
    ContinuityUnavailable,

    /// The store holds no history it can vouch for this client.
    #[error("the continuity store holds no accepted lifecycle for this client, so nothing was written")]
    ContinuityUnenrolled,

    /// The store's accepted lifecycle does not end at the document's.
    #[error("the client's recorded lifecycle does not match the continuity store's, so nothing was written")]
    ContinuityMismatch,

    /// The environment's placements could not be read.
    #[error("the environment's placements could not be read, so the retained placements cannot be checked")]
    PlacementsUnreadable,

    /// A placement was removed, lowered or orphaned while the client was
    /// retired.
    #[error("{detail}")]
    PlacementsNotRetained {
        /// Which, in an operator's words. Names and revisions only.
        detail: String,
    },

    /// Nothing in this process shows what was published, so the retained
    /// placements cannot be compared with it.
    #[error(
        "no publication pass has published since the control plane started, so the retained placements \
         cannot be checked against what was published"
    )]
    PlacementsUnverified,
}
