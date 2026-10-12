//! What an adapter read from its target, raw, and the one rule that decides
//! its completeness, so every adapter judges the same bytes the same way.

use super::{CompletenessGap, GapKind, PublicationCompleteness, RecordedPart, UnobservedReason};
use crate::digest::{sha256_digest, sha256_hex};
use crate::{DocumentKind, DocumentManifest, PublicationRecord, RecordStamp, RecordedDocument};

/// One document's bytes as held, unparsed. Either half may be absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservedDocument {
    /// The manifest's bytes.
    pub manifest: Option<Vec<u8>>,
    /// The payload's bytes.
    pub payload: Option<Vec<u8>>,
}

/// Everything one inspection read, with the record read first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetObservation {
    /// The publication record, read before anything else.
    pub record: Option<Vec<u8>>,
    /// `tenants.json` and its manifest.
    pub tenants: ObservedDocument,
    /// `data-sources.json` and its manifest.
    pub data_sources: ObservedDocument,
    /// `catalog.json` and its manifest.
    pub catalog: ObservedDocument,
    /// `issuer-registry.json`.
    pub registry: Option<Vec<u8>>,
}

impl TargetObservation {
    /// Judges this observation, given the record as read again after every
    /// other part. A record that moved in between means the parts may come
    /// from two publications, so nothing is concluded from them.
    #[must_use]
    pub fn assess(&self, record_after: Option<&[u8]>) -> PublicationCompleteness {
        if self.record.as_deref() != record_after {
            return unobserved(UnobservedReason::Unstable);
        }
        let Some(bytes) = self.record.as_deref() else {
            return PublicationCompleteness::Unpublished;
        };
        // Parsed without the rollback-floor check: that guards writing over
        // a record, and inspection writes nothing.
        let Ok(record) = serde_json::from_slice::<PublicationRecord>(bytes) else {
            return unobserved(UnobservedReason::RecordUnreadable);
        };
        let documents = record.documents();
        let mut gaps = Vec::new();
        document_gaps(
            &mut gaps,
            RecordedPart::Tenants,
            DocumentKind::Tenants,
            &self.tenants,
            &documents.tenants,
        );
        document_gaps(
            &mut gaps,
            RecordedPart::DataSources,
            DocumentKind::DataSources,
            &self.data_sources,
            &documents.data_sources,
        );
        document_gaps(
            &mut gaps,
            RecordedPart::Catalog,
            DocumentKind::Catalog,
            &self.catalog,
            &documents.catalog,
        );
        if let Some(registry) = record.registry() {
            payload_gap(
                &mut gaps,
                RecordedPart::IssuerRegistry,
                self.registry.as_deref(),
                &registry.sha256,
            );
        }

        let stamp = RecordStamp {
            generation: record.generation(),
            sha256: sha256_digest(bytes),
        };
        if gaps.is_empty() {
            PublicationCompleteness::Complete { record: stamp }
        } else {
            PublicationCompleteness::Incomplete { record: stamp, gaps }
        }
    }
}

const fn unobserved(reason: UnobservedReason) -> PublicationCompleteness {
    PublicationCompleteness::Unobserved { reason }
}

fn document_gaps(
    gaps: &mut Vec<CompletenessGap>,
    part: RecordedPart,
    kind: DocumentKind,
    observed: &ObservedDocument,
    recorded: &RecordedDocument,
) {
    let manifest = observed.manifest.as_deref().map(|bytes| {
        serde_json::from_slice::<DocumentManifest>(bytes)
            .ok()
            .filter(|manifest| manifest.document() == kind)
    });
    let manifest_gap = match manifest {
        None => Some(GapKind::ManifestMissing),
        Some(None) => Some(GapKind::ManifestUnreadable),
        Some(Some(manifest)) if manifest.revision() != recorded.revision => Some(GapKind::RevisionMismatch),
        Some(Some(_)) => None,
    };
    if let Some(kind) = manifest_gap {
        gaps.push(CompletenessGap { part, kind });
    }
    payload_gap(gaps, part, observed.payload.as_deref(), &recorded.sha256);
}

fn payload_gap(gaps: &mut Vec<CompletenessGap>, part: RecordedPart, payload: Option<&[u8]>, sha256: &str) {
    let kind = match payload {
        None => GapKind::PayloadMissing,
        Some(bytes) if sha256_hex(bytes) != sha256 => GapKind::DigestMismatch,
        Some(_) => return,
    };
    gaps.push(CompletenessGap { part, kind });
}
