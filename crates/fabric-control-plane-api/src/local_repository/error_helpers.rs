//! Small constructors that attach a path, or choose which of two near-twin
//! error variants a failure actually is.

use std::path::Path;

use fabric_client_model::DesiredStateError;
use fabric_control_plane::RepositoryError;

use super::LocalRepositoryError;

/// Builds the error for a failure a retry could plausibly fix.
pub(super) fn unavailable(detail: &str) -> RepositoryError {
    RepositoryError::Unavailable {
        detail: detail.into(),
    }
}

/// Builds the error for a document this store was asked to write and could
/// not even render — not [`unavailable`]: nothing about this document
/// becomes writable by asking again, so it is refused outright rather than
/// reported as a bad minute.
pub(super) fn rejected(detail: &str) -> RepositoryError {
    RepositoryError::Rejected {
        detail: detail.into(),
    }
}

/// Wraps an I/O failure with the path it happened against.
pub(super) fn io(path: &Path, source: std::io::Error) -> LocalRepositoryError {
    LocalRepositoryError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Wraps a snapshot JSON failure with the path it came from.
pub(super) fn invalid_snapshot(path: &Path, error: &serde_json::Error) -> LocalRepositoryError {
    LocalRepositoryError::InvalidSnapshot {
        path: path.to_path_buf(),
        detail: error.to_string(),
    }
}

/// Distinguishes a pre-envelope catalogue from every other way one can fail
/// to parse — see [`LocalRepositoryError::LegacyCatalogue`].
///
/// Only `UnknownDocumentKind { found: None, .. }` is the legacy shape: that
/// is the one case where the document carried neither `apiVersion` nor
/// `kind` at all, which is exactly what predates the envelope. A document
/// that carries *some* pair this build does not recognise — a newer
/// `apiVersion`, say — is not a legacy document; it is a different problem,
/// and saying "predates the versioned catalogue" about it would send an
/// operator looking for the wrong fix. That case falls through to
/// `InvalidCatalogue`, which keeps what was actually found.
pub(super) fn catalogue_error(path: &Path, source: DesiredStateError) -> LocalRepositoryError {
    if matches!(source, DesiredStateError::UnknownDocumentKind { found: None, .. }) {
        LocalRepositoryError::LegacyCatalogue {
            path: path.to_path_buf(),
            source,
        }
    } else {
        LocalRepositoryError::InvalidCatalogue {
            path: path.to_path_buf(),
            source,
        }
    }
}
