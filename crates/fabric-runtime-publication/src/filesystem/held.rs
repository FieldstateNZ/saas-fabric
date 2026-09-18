//! Reads whatever is currently on disk, before any write is attempted.
//!
//! Reading is all this file does: what the documents mean, and whether they
//! may be replaced, is decided by `crate::plan_publication`, which every
//! adapter shares.

use std::io;
use std::path::Path;

use super::paths::DocumentPaths;
use crate::{DocumentKind, DocumentManifest, HeldDocument, HeldDocuments, PublicationError};

/// Reads every held manifest and payload into the shape the plan decides
/// against. A missing file is `None`, not an error; anything else unreadable
/// is [`PublicationError::Unreadable`].
pub(super) fn read_held(
    tenants: &DocumentPaths,
    data_sources: &DocumentPaths,
    catalog: &DocumentPaths,
) -> Result<HeldDocuments, PublicationError> {
    Ok(HeldDocuments {
        tenants: read_document(tenants)?,
        data_sources: read_document(data_sources)?,
        catalog: read_document(catalog)?,
    })
}

fn read_document(paths: &DocumentPaths) -> Result<HeldDocument, PublicationError> {
    Ok(HeldDocument {
        manifest: read_manifest(&paths.manifest, paths.kind)?,
        payload: read_optional(&paths.payload, paths.kind)?,
    })
}

fn read_optional(path: &Path, document: DocumentKind) -> Result<Option<Vec<u8>>, PublicationError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(unreadable(document, error)),
    }
}

/// Reads and parses a document's manifest, if it exists.
///
/// Also checks the manifest's own `document` field against `document` --
/// the kind of the file it was read from. A manifest naming a different
/// document is refused as [`PublicationError::Unreadable`] rather than
/// trusted: it can only mean the file was copied or hand-edited into the
/// wrong place, and using it as-is would attribute the wrong document's
/// revision to this one.
fn read_manifest(path: &Path, document: DocumentKind) -> Result<Option<DocumentManifest>, PublicationError> {
    let Some(bytes) = read_optional(path, document)? else {
        return Ok(None);
    };

    let manifest: DocumentManifest =
        serde_json::from_slice(&bytes).map_err(|error| unreadable(document, error))?;

    if manifest.document() == document {
        Ok(Some(manifest))
    } else {
        Err(unreadable(
            document,
            ManifestKindMismatch {
                expected: document,
                found: manifest.document(),
            },
        ))
    }
}

/// A manifest's own `document` field does not match the file it sits beside.
#[derive(Debug, thiserror::Error)]
#[error("manifest claims to describe {found:?}, but was read as {expected:?}")]
struct ManifestKindMismatch {
    expected: DocumentKind,
    found: DocumentKind,
}

/// Wraps a read or parse failure as [`PublicationError::Unreadable`].
///
/// `pub(super)` because [`super::parse`] raises the same error for a
/// payload that reads fine as bytes but does not parse as the document it
/// claims to be.
pub(super) fn unreadable(
    document: DocumentKind,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> PublicationError {
    PublicationError::Unreadable {
        document,
        cause: Box::new(cause),
    }
}
