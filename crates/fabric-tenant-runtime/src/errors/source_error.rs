//! Why a set of resources could not be loaded from its source.

/// A source could not be read or understood.
///
/// A load failure never clears a registry. The last good snapshot keeps
/// serving, because a momentarily unreadable source is a far better reason to
/// serve slightly stale state than to reject every request.
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
}
