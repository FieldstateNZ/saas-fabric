//! Why the local development repository could not be opened.

use std::path::PathBuf;

use fabric_client_model::DesiredStateError;

/// Why [`LocalClientRepository::open`](super::LocalClientRepository::open)
/// could not open a store.
///
/// Typed rather than a `String`, for the same reason every other adapter's
/// startup failure is: a caller that wants to know *which* thing went wrong
/// — a lock already held, say, as opposed to a disk that is simply gone —
/// can match on this instead of pattern-matching a sentence. The host still
/// renders it as a string at the one place a startup failure is reported.
#[derive(Debug, thiserror::Error)]
pub enum LocalRepositoryError {
    /// A filesystem operation failed.
    #[error("could not access {path}: {source}")]
    Io {
        /// The path the operation was against.
        path: PathBuf,
        /// What the OS reported.
        #[source]
        source: std::io::Error,
    },

    /// Another process already holds this directory's lock.
    ///
    /// Its own variant rather than folded into [`Self::Io`], because it is
    /// the one failure here an operator can act on immediately — stop the
    /// other process — rather than a disk or permissions problem that needs
    /// investigating.
    #[error("another process already has {path} open")]
    AlreadyOpen {
        /// The lock file that is already held.
        path: PathBuf,
    },

    /// The stored snapshot is not valid JSON, or is JSON of the wrong shape.
    #[error("the stored snapshot at {path} is invalid: {detail}")]
    InvalidSnapshot {
        /// The state file.
        path: PathBuf,
        /// What was wrong with it.
        detail: String,
    },

    /// The snapshot's catalogue predates the versioned envelope.
    ///
    /// Not a document this model reads any more — see
    /// `fabric_client_model::catalogue::schema` — and not one this store
    /// migrates on an operator's behalf: deliberately no legacy read path.
    /// A `.fabric-state.json` this old was written before the envelope
    /// existed, and the fix is to remove it. The client `*.yaml` files are
    /// re-imported the next time this store opens; the catalogue has no
    /// such source to recover from, so a deployment that depended on one
    /// re-creates it through the console.
    #[error("{path} predates the versioned catalogue and must be removed or re-created: {source}")]
    LegacyCatalogue {
        /// The state file.
        path: PathBuf,
        /// What [`Catalogue::parse`](fabric_client_model::catalogue::Catalogue::parse) refused.
        #[source]
        source: DesiredStateError,
    },

    /// A stored catalogue will not parse, for a reason other than predating
    /// the envelope.
    #[error("{path} holds a stored catalogue that will not parse: {source}")]
    InvalidCatalogue {
        /// The state file.
        path: PathBuf,
        /// What [`Catalogue::parse`](fabric_client_model::catalogue::Catalogue::parse) refused.
        #[source]
        source: DesiredStateError,
    },

    /// A stored client document will not parse.
    #[error("{path} holds a client document that will not parse: {source}")]
    InvalidClient {
        /// The state file.
        path: PathBuf,
        /// What [`ClientDocument::parse`](fabric_client_model::ClientDocument::parse) refused.
        #[source]
        source: DesiredStateError,
    },
}
