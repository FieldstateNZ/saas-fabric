//! Reads whatever is currently on disk, before any write is attempted.

use std::io;
use std::path::Path;

use serde::de::DeserializeOwned;

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
        revision: manifest.revision,
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
fn read_manifest(path: &Path, document: DocumentKind) -> Result<Option<DocumentManifest>, PublicationError> {
    let Some(bytes) = read_optional(path, document)? else {
        return Ok(None);
    };

    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| unreadable(document, error))
}

/// Parses a held payload as a JSON array of `T`, treating an absent payload
/// as an empty set.
///
/// # Absent versus unparseable
///
/// Absent means nothing has been published yet — for the tenants document
/// specifically, read here to check ADR 0018 part 3's retirement rule, that
/// means no tenant has ever been bound, so there is nothing a data-sources
/// publication could possibly be retiring underneath. An empty set is the
/// correct and safe answer.
///
/// Unparseable is different, and is refused rather than guessed at: a held
/// file this producer wrote should always parse, so one that does not is
/// either corrupted or hand-edited into a state this code cannot vouch for.
/// Guessing "empty" would let a retirement past whatever it actually holds;
/// guessing "non-empty" would refuse forever. Both are wrong, so this
/// returns [`PublicationError::Unreadable`] and leaves the decision to an
/// operator before the next publication is attempted.
pub(super) fn parse_documents<T: DeserializeOwned>(
    payload: Option<&[u8]>,
    document: DocumentKind,
) -> Result<Vec<T>, PublicationError> {
    match payload {
        None => Ok(Vec::new()),
        Some(bytes) => serde_json::from_slice(bytes).map_err(|error| unreadable(document, error)),
    }
}

fn unreadable(
    document: DocumentKind,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> PublicationError {
    PublicationError::Unreadable {
        document,
        cause: Box::new(cause),
    }
}
