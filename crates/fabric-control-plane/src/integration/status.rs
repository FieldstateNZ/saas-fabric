//! How the platform's connection to desired state is reported.

use serde::Serialize;

/// The state of this platform's connection to client desired state.
///
/// Four, and the distinctions are the ones that change what an operator
/// should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStatus {
    /// Nothing has been connected yet. The console offers to connect one.
    NotConfigured,

    /// A repository is connected and the last read of it succeeded.
    Connected,

    /// Connected, but the platform's credential is being refused.
    ///
    /// Distinct from [`Error`](Self::Error) because the remedy is specific and
    /// an operator can perform it: reconnect. A revoked or removed
    /// installation lands here, which is the case this platform cannot be
    /// *told* about — the operator plane is a tailnet, so there is no webhook
    /// — and therefore has to notice for itself.
    Invalid,

    /// Connected, and reads are failing for some other reason.
    ///
    /// The honest bucket. It covers a repository that is unreachable, one
    /// refusing the request, and a document that will not parse — none of
    /// which an operator fixes by reconnecting, and all of which need somebody
    /// to look.
    Error,
}

impl IntegrationStatus {
    /// Whether this status describes a working connection.
    #[must_use]
    pub const fn is_healthy(self) -> bool {
        matches!(self, Self::Connected)
    }
}
