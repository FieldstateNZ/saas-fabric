//! What an adapter holds, in the shape the plan decides against.

use crate::verdict::Held;
use crate::{DocumentManifest, DocumentRevision, PublicationRecord, PublishedRevisions};

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

    /// What the verdict decides against: the manifest's revision, or --
    /// when the manifest is lost but the publication record still names
    /// this document -- the record's. Without that fallback a lost manifest
    /// read as a first publication, and an offer at revision 1 was accepted
    /// over a document published at a higher one.
    pub(super) fn held(&self, recorded: Option<DocumentRevision>) -> Option<Held<'_>> {
        self.manifest
            .as_ref()
            .map(DocumentManifest::revision)
            .or(recorded)
            .map(|revision| Held {
                revision,
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
    /// The publication record's bytes exactly as held, if one has ever been
    /// written. Parsed by [`plan_publication`](crate::plan_publication), not
    /// by the adapter, so every adapter enforces its floor the same way.
    pub record: Option<Vec<u8>>,
}

impl HeldDocuments {
    /// The revision each document is held at, as
    /// [`RuntimePublication::current`](crate::RuntimePublication::current)
    /// reports it: the manifest's, or the publication record's where the
    /// manifest is lost, and `None` where neither was ever published. A
    /// record that does not parse contributes nothing here; planning
    /// refuses it.
    #[must_use]
    pub fn revisions(&self) -> PublishedRevisions {
        let record = self
            .record
            .as_deref()
            .and_then(|bytes| PublicationRecord::parse_held(bytes).ok());
        let recorded = |pick: fn(&PublicationRecord) -> DocumentRevision| record.as_ref().map(pick);
        let held = |document: &HeldDocument, recorded: Option<DocumentRevision>| {
            document
                .manifest
                .as_ref()
                .map(DocumentManifest::revision)
                .or(recorded)
        };
        PublishedRevisions {
            tenants: held(&self.tenants, recorded(|r| r.documents().tenants.revision)),
            data_sources: held(
                &self.data_sources,
                recorded(|r| r.documents().data_sources.revision),
            ),
            catalog: held(&self.catalog, recorded(|r| r.documents().catalog.revision)),
        }
    }
}
