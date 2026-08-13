//! Why a set of resources could not be loaded from its source.

/// A source produced no usable state — it could not be read, could not be
/// understood, or held nothing that survived validation.
///
/// A load failure never clears a registry. The last good snapshot keeps
/// serving, because a momentarily unreadable source is a far better reason to
/// serve slightly stale state than to reject every request. On a *first* load
/// there is no snapshot to keep, so the registry is instead left unprimed —
/// returning 503 rather than pretending to be ready over an empty set.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// The source could not be read.
    ///
    /// The description field is named `origin` rather than `source` because
    /// `thiserror` treats a field called `source` as the error cause, and here
    /// it is only a human-readable label.
    #[error("could not read from {origin}")]
    Unreadable {
        /// A description of the source, for logs.
        origin: String,
        /// The underlying cause.
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The source was read but its contents could not be understood.
    #[error("contents of {origin} are malformed: {detail}")]
    Malformed {
        /// A description of the source, for logs.
        origin: String,
        /// What was wrong.
        detail: String,
    },

    /// The source parsed, but **every** resource in it failed validation, so
    /// there was nothing to install.
    ///
    /// Only ever produced by a *first* load
    /// ([`ResourceRefresher::prime`](crate::ResourceRefresher::prime)). A
    /// refresh cannot reach this state: by then a previous copy exists for
    /// every held key and is retained, so the registry keeps serving.
    ///
    /// It is a `SourceError` rather than a category of its own because that is
    /// what it is from the registry's side — a source that produced no usable
    /// state — and because it must reach the same decision the other variants
    /// do, [`fail_fast_on_prime`](crate::RuntimeConfig::fail_fast_on_prime).
    #[error("all {count} resources from {origin} failed validation; first was {reason}")]
    NothingUsable {
        /// A description of the source, for logs.
        origin: String,
        /// How many resources were published, every one of them unusable.
        count: usize,
        /// The first rejection, named, so the log says what to go and fix
        /// rather than only that something is wrong.
        reason: String,
    },
}
