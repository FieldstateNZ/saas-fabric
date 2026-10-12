//! A stored projected client that no longer matches its projection
//! (D01-19b). Computed by `ClientDocument::projection_divergence`.

use crate::OidcClientId;

/// Which part of a stored projected client no longer matches the projection.
///
/// Names a field, never a value: a callback is not echoed into an error, a
/// log line or an audit record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectedClientField {
    /// The projected entry is not in `spec.identity.clients` at all.
    Missing,
    /// Its protocol differs.
    Protocol,
    /// Its PKCE method differs.
    Pkce,
    /// Its redirect strategy differs.
    RedirectStrategy,
    /// Its set of callbacks differs: one added, removed or changed.
    RedirectUris,
}

impl ProjectedClientField {
    /// A stable name for API errors and logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Protocol => "protocol",
            Self::Pkce => "pkce",
            Self::RedirectStrategy => "redirectStrategy",
            Self::RedirectUris => "redirectUris",
        }
    }
}

/// A stored projected client that no longer matches what the stored
/// document projects. A fact about the document, not a claim about who
/// edited it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedClientDivergence {
    /// The projected application client.
    pub client: OidcClientId,
    /// The first part found to differ.
    pub field: ProjectedClientField,
}
