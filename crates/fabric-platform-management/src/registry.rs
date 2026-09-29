//! The port through which published artifacts are read.

mod attached;
mod resolved;

pub use attached::{Attached, AttachedDescriptor, Unusable};
pub use resolved::{Provenance, Resolved};

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

    /// The registry's realm refused the credential an operator gave for it.
    ///
    /// # Why its own variant, and not `Refused`
    ///
    /// It is not retryable, and not the request's fault either: asking again
    /// presents the same credential to the same realm, and a sweep that did
    /// that every minute would walk an account into a lockout. So the adapter
    /// marks the credential refused and fails every request that would
    /// present it with this, without contacting anything, until an operator
    /// replaces it (ADR 0026 section 5). The control plane's registry routes
    /// answer it `502` — an upstream that said no to what it was given —
    /// rather than a `503` that invites a retry; Platform Management's own
    /// routes treat it as any registry that could not answer.
    #[error("the registry refused its credential: {detail}")]
    Denied {
        /// What was refused, and by which step — never the username, the
        /// secret or the realm's response.
        detail: String,
    },
}

/// Somewhere published artifacts can be looked up.
///
/// Implemented by an adapter that speaks a registry's protocol. Nothing here
/// says which registry, or how it is authenticated to: the registry is its own
/// integration, and treating the platform repository's credential as the
/// registry's would conflate two things that must stay separable.
///
/// # Every digest is one the adapter computed
///
/// A digest this port hands back — [`Resolved::digest`],
/// [`AttachedDescriptor::digest`] — is the SHA-256 of bytes the adapter read
/// and hashed itself (ADR 0026 section 3). A registry's `Docker-Content-Digest`
/// header is a pointer an adapter checks, never a fact it reports: what gets
/// pinned into desired state is what was proven, not what was claimed.
#[async_trait::async_trait]
pub trait Registry: Send + Sync {
    /// Every tag published for a repository.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked.
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError>;

    /// What one reference resolves to, or `None` if there is no such thing.
    ///
    /// `reference` is a tag, or a digest written `sha256:<64 hex>`: the same
    /// question either way — *which bytes, and where do they say they came
    /// from* — and asking it of a digest is how a caller proves an image a
    /// component descriptor names exists. Only `sha256` is accepted; a digest
    /// in any other algorithm is [`Refused`](RegistryError::Refused), because
    /// a digest nobody here can compute is one nobody here can check.
    ///
    /// Absence is an answer rather than an error, and that is load-bearing.
    /// A component's images are published by parallel jobs, so a version
    /// existing in one repository and not yet in another is an ordinary
    /// minutes-long window — not a fault, and not something to remember.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked, or if what it
    /// served does not hash to the digest it was asked for or claimed.
    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError>;

    /// The component descriptor attached to `subject`, a `sha256:<64 hex>`
    /// digest in `repository` — read by the rules [`Attached`] documents
    /// (ADR 0026 section 4).
    ///
    /// The document comes back as bytes, unparsed. Whether they are a
    /// component descriptor Fabric reads is a question for the domain, which
    /// owns the contract; an adapter that parsed them would be a second
    /// reader of a contract that must have one.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked — a timeout, a
    /// `429` or `5xx`, or a `401`/`403` after a token was issued, none of
    /// which may read as *nothing attached* — if the repository does not
    /// exist, if a subject not written as a `sha256` digest was asked
    /// about, or if bytes did not hash to their digest.
    async fn component_descriptor(&self, repository: &str, subject: &str) -> Result<Attached, RegistryError>;
}
