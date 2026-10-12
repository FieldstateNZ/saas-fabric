//! A stored projected client that no longer matches its projection
//! (D01-19b), and what a stored document's projection check concludes.
//! Computed by `ClientDocument::projection_check`.

use crate::OidcClientId;

/// Which part of a stored projected client no longer matches the projection.
///
/// Names a field, never a value: a callback is not echoed into an error, a
/// log line, an audit record or a read response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// A stable name for API errors and logs. The same as the serialised
    /// form.
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProjectedClientDivergence {
    /// The projected application client.
    pub client: OidcClientId,
    /// The first part found to differ.
    pub field: ProjectedClientField,
}

/// Whether a stored document's projected entries are still what the
/// document itself projects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionCheck {
    /// Every previously assigned projected entry matches its projection.
    Consistent,

    /// These stored projected entries differ from their projection, one per
    /// client.
    Diverged(Vec<ProjectedClientDivergence>),

    /// The stored document's projection cannot be computed: its product
    /// section does not parse, or its assignments no longer project (no
    /// host and no template, say). Nothing can be said about which stored
    /// entries are the projection's.
    Unverifiable {
        /// The declared clients that may be projected entries: the assigned
        /// applications that are declared, or every declared client when the
        /// assignments themselves cannot be read.
        unverified: Vec<OidcClientId>,
    },
}

impl ProjectionCheck {
    /// The declared clients a projection-based convergence must not
    /// converge: a diverged entry is not the projection, and an unverified
    /// one cannot be shown to be. A `Missing` entry is not declared, so
    /// there is nothing to withhold.
    #[must_use]
    pub fn withheld(&self) -> Vec<OidcClientId> {
        match self {
            Self::Consistent => Vec::new(),
            Self::Diverged(divergences) => divergences
                .iter()
                .filter(|divergence| divergence.field != ProjectedClientField::Missing)
                .map(|divergence| divergence.client.clone())
                .collect(),
            Self::Unverifiable { unverified } => unverified.clone(),
        }
    }
}
