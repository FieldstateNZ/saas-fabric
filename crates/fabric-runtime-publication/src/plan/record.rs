//! Deciding the publication record a plan ends with.

use crate::{
    DocumentKind, DocumentOutcome, DocumentPlan, PublicationError, PublicationRecord, RecordedDocument,
    RecordedDocuments, ResourceHistory,
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
}

/// Names each planned document's bytes and revision, and the resource
/// history including them, and advances the generation when that differs
/// from the held record.
///
/// A held record left by an interrupted publication names documents that may
/// not all have landed; it is still the generation readers serve, so the
/// next one is numbered after it. With no held record -- a first
/// publication, or a set written before the record existed -- the generation
/// starts at 1.
pub(super) fn decide(
    held: Option<PublicationRecord>,
    data_sources: &DocumentPlan,
    catalog: &DocumentPlan,
    tenants: &DocumentPlan,
    resources: ResourceHistory,
) -> Result<RecordPlan, PublicationError> {
    let documents = RecordedDocuments {
        tenants: RecordedDocument::of(tenants.revision, &tenants.bytes),
        data_sources: RecordedDocument::of(data_sources.revision, &data_sources.bytes),
        catalog: RecordedDocument::of(catalog.revision, &catalog.bytes),
    };

    let (outcome, record) = match held {
        Some(held)
            if PublicationRecord::new(held.generation(), documents.clone(), resources.clone()) == held =>
        {
            (DocumentOutcome::Unchanged, held)
        }
        Some(held) => (
            DocumentOutcome::Written,
            PublicationRecord::new(held.generation().saturating_add(1), documents, resources),
        ),
        None => (
            DocumentOutcome::Written,
            PublicationRecord::new(1, documents, resources),
        ),
    };

    let bytes = record
        .canonical_json()
        .map_err(|cause| PublicationError::Unwritable {
            document: DocumentKind::Tenants,
            cause: Box::new(cause),
        })?;

    Ok(RecordPlan {
        outcome,
        record,
        bytes,
    })
}
