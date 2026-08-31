//! The port through which available versions are discovered.

/// What went wrong asking a registry.
///
/// Deliberately small. A registry that cannot be reached leaves availability
/// *stale*, and stale availability is not a failure of desired state — nothing
/// is written, and what an environment is asked to run does not change.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// The registry could not be reached, or failed internally.
    #[error("the registry is unavailable: {detail}")]
    Unavailable {
        /// What was observed, with no upstream body and no credential in it.
        detail: String,
    },

    /// The registry refused the request.
    #[error("the registry refused the request: {detail}")]
    Refused {
        /// What was observed, with no upstream body and no credential in it.
        detail: String,
    },
}

/// What a registry knows about one tag of one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The manifest digest, which is what a deployment pins.
    pub digest: String,

    /// `org.opencontainers.image.revision` — the commit it was built from.
    ///
    /// Optional because an image may carry no such label, and an image whose
    /// provenance cannot be read is one this crate will not promote. That is a
    /// refusal, not a crash.
    pub revision: Option<String>,
}

/// Somewhere published artifacts can be looked up.
///
/// Implemented by an adapter that speaks a registry's protocol. Nothing here
/// says which registry, or how it is authenticated to: the registry is its own
/// integration, and treating the platform repository's credential as the
/// registry's would conflate two things that must stay separable.
#[async_trait::async_trait]
pub trait Registry: Send + Sync {
    /// Every tag published for a repository.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked.
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError>;

    /// What one tag resolves to, or `None` if there is no such tag.
    ///
    /// Absence is an answer rather than an error, and that is load-bearing.
    /// A component's images are published by parallel jobs, so a version
    /// existing in one repository and not yet in another is an ordinary
    /// minutes-long window — not a fault, and not something to remember.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked.
    async fn resolve(&self, repository: &str, tag: &str) -> Result<Option<Resolved>, RegistryError>;
}
