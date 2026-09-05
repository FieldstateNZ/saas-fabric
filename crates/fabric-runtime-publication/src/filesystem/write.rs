//! Writes one document to disk, once its verdict says it must be.

use super::atomic_write::atomic_write;
use super::paths::DocumentPaths;
use crate::verdict::Verdict;
use crate::{DocumentKind, DocumentManifest, DocumentRevision, PublicationError, CONTRACT_VERSION};

/// Writes one document's payload, then its manifest, unless its verdict says
/// nothing changed.
///
/// A free function, not a method: it reaches only `paths`, `verdict`,
/// `bytes`, and `revision` — never the adapter itself — so nothing here can
/// drift into touching a *different* document's paths by accident.
pub(super) fn write_if_needed(
    paths: &DocumentPaths,
    verdict: Verdict,
    bytes: &[u8],
    revision: DocumentRevision,
) -> Result<(), PublicationError> {
    if verdict == Verdict::Unchanged {
        return Ok(());
    }

    atomic_write(&paths.payload, bytes).map_err(|cause| unwritable(paths.kind, cause))?;

    let manifest = DocumentManifest {
        contract_version: CONTRACT_VERSION,
        document: paths.kind,
        revision,
    };
    let manifest_bytes =
        crate::canonical::to_canonical_bytes(&manifest).map_err(|cause| unwritable(paths.kind, cause))?;

    atomic_write(&paths.manifest, &manifest_bytes).map_err(|cause| unwritable(paths.kind, cause))
}

fn unwritable(
    document: DocumentKind,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> PublicationError {
    PublicationError::Unwritable {
        document,
        cause: Box::new(cause),
    }
}
