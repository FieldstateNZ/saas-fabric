//! Writes one document to disk, once its verdict says it must be.

use std::path::Path;

use super::atomic_write::atomic_write;
use super::paths::DocumentPaths;
use crate::{
    DocumentKind, DocumentManifest, DocumentOutcome, DocumentRevision, PublicationError, RecordPlan,
    RegistryPlan,
};

/// Writes one document's payload, then its manifest, unless its verdict says
/// nothing changed.
///
/// A free function, not a method: it reaches only `paths`, `verdict`,
/// `bytes`, and `revision` — never the adapter itself — so nothing here can
/// drift into touching a *different* document's paths by accident.
pub(super) fn write_if_needed(
    paths: &DocumentPaths,
    outcome: DocumentOutcome,
    bytes: &[u8],
    revision: DocumentRevision,
) -> Result<(), PublicationError> {
    if outcome == DocumentOutcome::Unchanged {
        return Ok(());
    }

    atomic_write(&paths.payload, bytes).map_err(|cause| unwritable(paths.kind, cause))?;

    let manifest = DocumentManifest::new(paths.kind, revision);
    let manifest_bytes = manifest
        .canonical_json()
        .map_err(|cause| unwritable(paths.kind, cause))?;

    atomic_write(&paths.manifest, &manifest_bytes).map_err(|cause| unwritable(paths.kind, cause))
}

/// Writes the publication record, after every document, unless the plan
/// left it unchanged. A failure is reported against the tenants document,
/// which the record travels with.
pub(super) fn write_record_if_needed(path: &Path, plan: &RecordPlan) -> Result<(), PublicationError> {
    if plan.outcome == DocumentOutcome::Unchanged {
        return Ok(());
    }
    atomic_write(path, &plan.bytes).map_err(|cause| unwritable(DocumentKind::Tenants, cause))
}

/// Writes the issuer registry, after the tenants it names and before the
/// record, unless its bytes are already the held ones. Reported against the
/// tenants document, which it travels with.
pub(super) fn write_registry_if_needed(path: &Path, plan: &RegistryPlan) -> Result<(), PublicationError> {
    if plan.outcome == DocumentOutcome::Unchanged {
        return Ok(());
    }
    atomic_write(path, &plan.bytes).map_err(|cause| unwritable(DocumentKind::Tenants, cause))
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
