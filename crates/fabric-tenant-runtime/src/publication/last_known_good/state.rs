//! What a reader's last-known-good copy holds, as readiness reports it.

/// The last-known-good copy's state, for readiness and the recovery command.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LastKnownGoodState {
    /// No last-known-good directory is configured: a replica restarting
    /// while the published files are incomplete fails its prime, as before.
    Disabled,
    /// Configured, and nothing persisted yet.
    Absent,
    /// A copy of this generation is persisted and validated.
    Stored {
        /// Its generation.
        generation: u64,
    },
    /// A copy exists and was refused; it is not served.
    Refused {
        /// Why.
        reason: String,
    },
    /// The last attempt to persist a generation failed. The copy on disk,
    /// if any, is whatever an earlier write left whole.
    PersistFailed {
        /// The generation that could not be persisted.
        generation: u64,
        /// Why.
        reason: String,
    },
}
