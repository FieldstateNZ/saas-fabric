//! What the last inspection of the publication target found, and when (G3).

use fabric_runtime_publication::PublicationCompleteness;

/// One inspection of the target, recorded by the pass that made it.
///
/// Kept apart from [`crate::LastPass`]: a pass can fail without writing
/// anything and leave the target complete, and a pass can be accepted
/// after an earlier one left it incomplete. Nothing but another inspection
/// replaces it, so neither a restart nor elapsed time makes an incomplete
/// target read as complete: a restart starts with no observation at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletenessObservation {
    /// When the pass that inspected it finished, as seconds since the Unix
    /// epoch.
    pub at_unix_seconds: u64,
    /// What it found.
    pub completeness: PublicationCompleteness,
}
