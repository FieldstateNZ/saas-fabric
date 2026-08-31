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

/// What an artifact says about where it came from.
///
/// # Why absence and disagreement are not the same answer
///
/// An artifact carrying no revision may simply still be publishing — a push
/// in flight looks identical to a label that was never set, and waiting is the
/// cheaper mistake. An artifact whose parts *disagree* about their source
/// commit is one version built twice, and no amount of waiting resolves it.
///
/// Collapsing them would either retry a broken build forever or refuse a
/// perfectly ordinary publishing window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// Everything inspected agrees it came from this commit.
    Agreed(String),

    /// Something inspected carries no revision at all.
    Absent,

    /// The parts inspected name different commits.
    Disagreed,
}

/// What a registry knows about one tag of one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The manifest digest, which is what a deployment pins.
    ///
    /// For a multi-architecture image this is the **index**, so a node still
    /// selects its own architecture. Pinning one platform's manifest would
    /// hand every node the same one.
    pub digest: String,

    /// Where it says it came from.
    ///
    /// An adapter reporting this for an index must satisfy itself that *every*
    /// manifest it inspected agrees. Reading one platform's label proves that
    /// platform's provenance and not the artifact's, and "the architecture we
    /// happen to run today" is not a fact about the image.
    pub provenance: Provenance,
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
