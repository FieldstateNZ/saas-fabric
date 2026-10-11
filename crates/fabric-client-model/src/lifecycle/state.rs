//! The four lifecycle states, without what each carries.

/// The four states, without what each carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleState {
    /// Saved intent: the document exists, the client is not usable yet.
    Incomplete,
    /// Provisioned, identity applied and published.
    Ready,
    /// Taken out of service, with its state retained.
    Tombstoned,
    /// Deletion confirmed. Records intent only.
    DeletionConfirmed,
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Incomplete => "incomplete",
            Self::Ready => "ready",
            Self::Tombstoned => "tombstoned",
            Self::DeletionConfirmed => "deletion-confirmed",
        })
    }
}
