//! Where the derived runtime catalogue comes from, without this crate
//! depending on `fabric-client-model`.

use fabric_runtime_publication::CatalogDocument;

/// Reads the environment's derived runtime catalogue (ADR 0023 part 3).
///
/// # Why this is a port, and not a direct call
///
/// `Catalogue::runtime_catalogue` lives in `fabric-client-model`, which this
/// crate must not depend on: `fabric-platform-management` is the platform's
/// rules half, and gains no edge to the client desired-state crate
/// (`scripts/check_architecture.py`). The control plane already depends on
/// both, so it implements this trait over its own `Catalogue` (ADR 0023
/// part 4, D4 -- not built here); [`RuntimePublisher`](crate::RuntimePublisher)
/// composes against the trait alone.
#[async_trait::async_trait]
pub trait RuntimeCatalogueSource: Send + Sync {
    /// The environment's complete derived catalogue.
    ///
    /// # Errors
    ///
    /// [`CatalogueSourceError`] if two applications' published releases
    /// declare the same resource name, or if the catalogue could not be
    /// read at all.
    async fn runtime_catalogue(&self) -> Result<CatalogDocument, CatalogueSourceError>;
}

/// Why [`RuntimeCatalogueSource::runtime_catalogue`] could not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogueSourceError {
    /// Two different applications' published releases declare the same
    /// resource name.
    ///
    /// ADR 0023 part 3 refuses this at release publication -- a draft never
    /// owns a name, and the second application's release is refused when
    /// published, naming both applications. Reaching it here means a
    /// conflict that predates that guard, or a hand edit to desired state.
    /// Never resolved by guessing a winner: a publication pass refuses and
    /// names both applications, the same way release publication does.
    #[error("{resource} is declared by both {} and {}", applications.0, applications.1)]
    Conflict {
        /// The resource name two applications both declare.
        resource: String,
        /// The two applications' names, in no particular order.
        applications: (String, String),
    },

    /// The catalogue could not be read at all -- a repository outage, not a
    /// conflict in what was read.
    #[error("{0}")]
    Unavailable(String),
}
