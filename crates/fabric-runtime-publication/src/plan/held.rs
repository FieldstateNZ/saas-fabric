//! What an adapter holds, in the shape the plan decides against.

use crate::verdict::Held;
use crate::{DocumentManifest, PublishedRevisions};

/// What an adapter currently holds for one document.
///
/// Either half may be absent on its own: a payload with no manifest is
/// content that was never published through the port and is still honoured
/// for referential checks, and a manifest with no payload is a document
/// whose bytes went missing, which [`plan_publication`](crate::plan_publication)
/// refuses as [`PublicationError::HeldPayloadLost`](crate::PublicationError::HeldPayloadLost).
#[derive(Debug, Clone, Default)]
pub struct HeldDocument {
    /// The sidecar manifest, if one has ever been published.
    pub manifest: Option<DocumentManifest>,
    /// The payload bytes exactly as held, if present.
    pub payload: Option<Vec<u8>>,
}

impl HeldDocument {
    /// Nothing held at all.
    #[must_use]
    pub const fn absent() -> Self {
        Self {
            manifest: None,
            payload: None,
        }
    }

    pub(super) fn held(&self) -> Option<Held<'_>> {
        self.manifest.as_ref().map(|manifest| Held {
            revision: manifest.revision(),
            payload: self.payload.as_deref(),
        })
    }
}

/// Everything an adapter holds, for all three documents, read once before
/// a publication is planned.
#[derive(Debug, Clone, Default)]
pub struct HeldDocuments {
    /// `tenants.json` and its manifest.
    pub tenants: HeldDocument,
    /// `data-sources.json` and its manifest.
    pub data_sources: HeldDocument,
    /// `catalog.json` and its manifest.
    pub catalog: HeldDocument,
}

impl HeldDocuments {
    /// The revision each document is held at, as
    /// [`RuntimePublication::current`](crate::RuntimePublication::current)
    /// reports it: `None` where no manifest has ever been published.
    #[must_use]
    pub fn revisions(&self) -> PublishedRevisions {
        PublishedRevisions {
            tenants: self.tenants.manifest.as_ref().map(DocumentManifest::revision),
            data_sources: self
                .data_sources
                .manifest
                .as_ref()
                .map(DocumentManifest::revision),
            catalog: self.catalog.manifest.as_ref().map(DocumentManifest::revision),
        }
    }
}
