//! What the continuity store hands back: a reservation, or a refusal.

/// A reservation the store handed out. Opaque to the control plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuityReservation(String);

impl ContinuityReservation {
    /// A reservation the store names `id`.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The store's name for it.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.0
    }
}

/// Why the store would not reserve or complete a transition.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContinuityRefusal {
    /// The store holds no history for this client.
    #[error("the continuity store holds no accepted lifecycle for this client")]
    Unenrolled,
    /// The transition does not follow what the store accepted: another
    /// instance, sequence or lifecycle.
    #[error("the transition does not follow the continuity store's accepted lifecycle")]
    Mismatch,
    /// A different transition is reserved and not completed.
    #[error("another lifecycle transition for this client is pending")]
    Pending,
    /// A creation names a client the store already knows.
    #[error("the continuity store already knows this client; a new instance would resurrect it")]
    Resurrection,
    /// A completion that is not the pending reservation's.
    #[error("the completion does not match the pending reservation")]
    StaleCompletion,
    /// The store could not be reached or did not answer.
    #[error("the continuity store is unavailable: {detail}")]
    Unavailable {
        /// What failed, for the log; never shown to a browser.
        detail: String,
    },
}
