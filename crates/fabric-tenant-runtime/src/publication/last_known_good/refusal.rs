//! Why a persisted last-known-good copy is not used.

/// A last-known-good file that exists but may not be served.
///
/// A refused copy is never repaired, rewritten from memory, or deleted by
/// the reader. It is left where it is for an operator to inspect and, if
/// they choose, discard (`fabric-api last-known-good discard`). The next
/// published generation that validates replaces it.
#[derive(Debug, thiserror::Error)]
pub enum LastKnownGoodRefusal {
    /// The file could not be read.
    #[error("could not read {path}: {cause}")]
    Unreadable {
        /// The file.
        path: String,
        /// The underlying cause.
        cause: std::io::Error,
    },

    /// The file, its record, or a document in it does not parse.
    #[error("last-known-good is malformed: {detail}")]
    Malformed {
        /// What was wrong.
        detail: String,
    },

    /// The file was written in a format this reader does not know.
    #[error("last-known-good format {found} is not one this reader reads")]
    UnsupportedFormat {
        /// The format the file states.
        found: u32,
    },

    /// A document, or the record, is not the bytes its digest names.
    #[error("last-known-good {document} does not match its digest")]
    DigestMismatch {
        /// `record`, `catalog`, `data-sources` or `tenants`.
        document: &'static str,
    },

    /// The copy's record requires a newer reader than this one. A binary
    /// rolled back below the copy may not serve it, exactly as it may not
    /// serve the published generation.
    #[error("last-known-good requires reader contract {required}; this reader is {supported}")]
    UnsupportedContract {
        /// The record's `minimum_reader_contract`.
        required: u32,
        /// This reader's contract.
        supported: u32,
    },
}
