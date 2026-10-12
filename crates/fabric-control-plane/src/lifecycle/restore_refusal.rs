//! Why a restore was refused by the retained-placement check before anything
//! was written (D01-14 restore).

/// The client's placements are not shown to be as it left them.
///
/// Decided before the continuity reservation and the document write:
/// nothing is reserved, written, audited or reconciled after one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RestoreRefusal {
    /// The environment's placements, or what was published from them, could
    /// not be read.
    #[error("the placements, or what was published from them, could not be read, so the retained placements cannot be checked")]
    PlacementsUnreadable,

    /// A placement was removed, replaced, lowered or orphaned while the
    /// client was retired.
    #[error("{detail}")]
    PlacementsNotRetained {
        /// Which, in an operator's words. Names and revisions only.
        detail: String,
    },

    /// Nothing establishes what was published: no platform or publication
    /// target is configured, or the target holds no accepted publication.
    #[error(
        "nothing establishes what was published for this client, so the retained placements cannot be \
         checked"
    )]
    PlacementsUnverified,
}
