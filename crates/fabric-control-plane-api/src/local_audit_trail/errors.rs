//! Why the local audit trail could not be opened.

use std::path::PathBuf;

use fabric_control_plane::AuditError;

/// Why [`LocalAuditTrail::open`](super::LocalAuditTrail::open) refused.
///
/// Opening verifies every stream, so a trail that has been edited, cut short
/// or reordered is refused at startup rather than extended — a new entry
/// chained onto a broken stream would lend it a valid head.
#[derive(Debug, thiserror::Error)]
pub enum LocalAuditTrailError {
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
    #[error("another process already has the audit trail at {path} open")]
    AlreadyOpen {
        /// The lock file that is already held.
        path: PathBuf,
    },

    /// A file in the directory names no audit stream.
    #[error("{path} is not an audit stream's log or head")]
    Unrecognised {
        /// The file.
        path: PathBuf,
    },

    /// A stream could not be read, or does not verify.
    #[error("the audit trail at {path} cannot be trusted: {source}")]
    Stream {
        /// The trail's directory.
        path: PathBuf,
        /// What was wrong.
        #[source]
        source: AuditError,
    },
}
