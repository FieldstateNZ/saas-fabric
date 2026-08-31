//! What the token endpoint answers with.

/// What the installation-token endpoint returns.
///
/// The one wire type here carrying a secret. It is never logged and never
/// returned upward except as the bearer itself.
#[derive(serde::Deserialize)]
pub(super) struct InstallationToken {
    /// The bearer to present to the host's API.
    pub(super) token: String,

    /// When the host says it stops working, as an RFC 3339 timestamp.
    ///
    /// Read rather than assumed. An earlier version cached every token for a
    /// fixed fifty minutes on the grounds that GitHub issues them for an hour
    /// — which is true today, is not a promise, and left the platform holding
    /// a dead token for the remainder of the window if it ever stopped being
    /// true.
    pub(super) expires_at: String,
}

// No `Debug` on the type above: deriving one would put an installation token
// into any error or log line that formatted a value containing it.
