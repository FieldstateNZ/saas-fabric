//! Deciding the publication record a plan ends with.

use crate::digest::sha256_digest;
use crate::{
    DocumentKind, DocumentOutcome, DocumentPlan, PublicationError, PublicationRecord, RecordStamp,
    RecordedDocument, RecordedDocuments, RegistryPlan, ResourceHistory,
};

/// The record's decided fate: written whenever the generation it names
/// differs from the one held, so a publication that changed nothing leaves
/// it, and its generation number, alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPlan {
    /// Whether the adapter must write it, after every document.
    pub outcome: DocumentOutcome,
    /// The record the published set is described by once this plan is
    /// written.
    pub record: PublicationRecord,
    /// Its canonical bytes.
    pub bytes: Vec<u8>,
    /// The record the target holds once this plan is written: the bytes
    /// written, or, when unchanged, the bytes already held, which a reader
    /// hashes as they are.
    pub stamp: RecordStamp,
}

/// Names each planned document's bytes and revision, the resource history
/// including them and the registry generation, and advances the generation when that differs
/// from the held record.
///
/// A held record left by an interrupted publication names documents that may
/// not all have landed; it is still the generation readers serve, so the
/// next one is numbered after it. With no held record -- a first
/// publication, or a set written before the record existed -- the generation
/// starts at 1.
pub(super) fn decide(
    held: Option<(PublicationRecord, &[u8])>,
    [data_sources, catalog, tenants]: [&DocumentPlan; 3],
    resources: ResourceHistory,
    registry: Option<&RegistryPlan>,
) -> Result<RecordPlan, PublicationError> {
    let registry = registry.map(RegistryPlan::recorded);
    let documents = RecordedDocuments {
        tenants: RecordedDocument::of(tenants.revision, &tenants.bytes),
        data_sources: RecordedDocument::of(data_sources.revision, &data_sources.bytes),
        catalog: RecordedDocument::of(catalog.revision, &catalog.bytes),
    };

    let (outcome, record, held_bytes) = match held {
        Some((held, held_bytes))
            if PublicationRecord::new(held.generation(), documents.clone(), resources.clone())
                .with_registry(registry.clone())
                == held =>
        {
            (DocumentOutcome::Unchanged, held, Some(held_bytes))
        }
        Some((held, _)) => (
            DocumentOutcome::Written,
            PublicationRecord::new(held.generation().saturating_add(1), documents, resources)
                .with_registry(registry),
            None,
        ),
        None => (
            DocumentOutcome::Written,
            PublicationRecord::new(1, documents, resources).with_registry(registry),
            None,
        ),
    };

    let bytes = record
        .canonical_json()
        .map_err(|cause| PublicationError::Unwritable {
            document: DocumentKind::Tenants,
            cause: Box::new(cause),
        })?;

    let stamp = RecordStamp {
        generation: record.generation(),
        sha256: sha256_digest(held_bytes.unwrap_or(&bytes)),
    };
    Ok(RecordPlan {
        outcome,
        record,
        bytes,
        stamp,
    })
}
