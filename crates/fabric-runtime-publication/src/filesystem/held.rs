//! Reads whatever is currently on disk, before any write is attempted.
//!
//! This file is in the 121-150 line band the file-size policy asks a reason
//! for: `HeldState` and the three small `read_*` functions it is built from
//! are one concept -- what does the filesystem currently hold, read once,
//! before any write is planned. Turning held bytes into typed documents is a
//! separate concept and lives in `super::parse` instead.

use std::io;
use std::path::Path;

use super::paths::DocumentPaths;
use crate::verdict::Held;
use crate::{DocumentKind, DocumentManifest, PublicationError};

/// Every fact about what is currently held, for all three documents, read
/// once before any write is attempted.
pub(super) struct HeldState {
    pub(super) tenants_manifest: Option<DocumentManifest>,
    pub(super) tenants_payload: Option<Vec<u8>>,
    pub(super) data_sources_manifest: Option<DocumentManifest>,
    pub(super) data_sources_payload: Option<Vec<u8>>,
    pub(super) catalog_manifest: Option<DocumentManifest>,
    pub(super) catalog_payload: Option<Vec<u8>>,
}

impl HeldState {
    /// Reads every held manifest and payload. A missing file is `None`, not
    /// an error; anything else unreadable is
    /// [`PublicationError::Unreadable`].
    pub(super) fn read(
        tenants: &DocumentPaths,
        data_sources: &DocumentPaths,
        catalog: &DocumentPaths,
    ) -> Result<Self, PublicationError> {
        Ok(Self {
            tenants_manifest: read_manifest(&tenants.manifest, tenants.kind)?,
            tenants_payload: read_optional(&tenants.payload, tenants.kind)?,
            data_sources_manifest: read_manifest(&data_sources.manifest, data_sources.kind)?,
            data_sources_payload: read_optional(&data_sources.payload, data_sources.kind)?,
            catalog_manifest: read_manifest(&catalog.manifest, catalog.kind)?,
            catalog_payload: read_optional(&catalog.payload, catalog.kind)?,
        })
    }

    /// The tenants document's held state, in the shape `verdict` compares
    /// against.
    pub(super) fn tenants_held(&self) -> Option<Held<'_>> {
        held_of(self.tenants_manifest.as_ref(), self.tenants_payload.as_deref())
    }

    /// The data-sources document's held state, in the shape `verdict`
    /// compares against.
    pub(super) fn data_sources_held(&self) -> Option<Held<'_>> {
        held_of(
            self.data_sources_manifest.as_ref(),
            self.data_sources_payload.as_deref(),
        )
    }

    /// The catalogue document's held state, in the shape `verdict` compares
    /// against.
    pub(super) fn catalog_held(&self) -> Option<Held<'_>> {
        held_of(self.catalog_manifest.as_ref(), self.catalog_payload.as_deref())
    }
}

/// `None` unless a manifest is held — an orphaned payload with no manifest
/// is the presence table's "first publication" row, not something
/// `verdict` compares bytes against.
fn held_of<'a>(manifest: Option<&DocumentManifest>, payload: Option<&'a [u8]>) -> Option<Held<'a>> {
    manifest.map(|manifest| Held {
        revision: manifest.revision(),
        payload,
    })
}

/// Reads a file's raw bytes, if it exists.
///
/// `None` means nothing has ever been published here — not an error, and
/// not the same as an empty file. Any other I/O failure is
/// [`PublicationError::Unreadable`].
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
