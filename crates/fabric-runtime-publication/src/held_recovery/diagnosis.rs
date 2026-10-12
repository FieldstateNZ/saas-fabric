//! What is held, and whether it blocks publication, read without writing
//! and without the plan's first-refusal-wins order, so an operator sees
//! every reason at once.

use super::held_parse::{blocking, parse_manifest};
use super::held_state::{DocumentDiagnosis, ManifestState, RecordState};
use crate::digest::sha256_hex;
use crate::{DocumentKind, ObservedDocument, PublicationRecord, RecordedDocument, TargetObservation};

/// The held state of a publication target, as the recovery command
/// reports it. Carries digests and revisions, never document content.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HeldDiagnosis {
    /// The publication record.
    pub record: RecordState,
    /// `tenants.json` and its manifest.
    pub tenants: DocumentDiagnosis,
    /// `data-sources.json` and its manifest.
    pub data_sources: DocumentDiagnosis,
    /// `catalog.json` and its manifest.
    pub catalog: DocumentDiagnosis,
    /// Every reason the held state refuses a publication before any offer
    /// is looked at; empty when a publication could be planned against it.
    pub blocked: Vec<String>,
}

/// Diagnoses what an adapter observed. Pure; writes nothing.
#[must_use]
pub fn diagnose(observation: &TargetObservation) -> HeldDiagnosis {
    let (record, parsed) = match observation.record.as_deref() {
        None => (RecordState::Absent, None),
        Some(bytes) => match PublicationRecord::parse_held(bytes) {
            Ok(parsed) => (
                RecordState::Held {
                    generation: parsed.generation(),
                    sha256: sha256_hex(bytes),
                },
                Some(parsed),
            ),
            Err(error) => (
                RecordState::Unusable {
                    detail: error.to_string(),
                    sha256: sha256_hex(bytes),
                },
                None,
            ),
        },
    };
    let documents = parsed.as_ref().map(PublicationRecord::documents);
    let tenants = document(
        DocumentKind::Tenants,
        &observation.tenants,
        documents.map(|d| &d.tenants),
    );
    let data_sources = document(
        DocumentKind::DataSources,
        &observation.data_sources,
        documents.map(|d| &d.data_sources),
    );
    let catalog = document(
        DocumentKind::Catalog,
        &observation.catalog,
        documents.map(|d| &d.catalog),
    );
    let mut blocked = Vec::new();
    if let RecordState::Unusable { detail, .. } = &record {
        blocked.push(format!("publication record: {detail}"));
    }
    for (name, diagnosis) in [
        ("tenants", &tenants),
        ("data sources", &data_sources),
        ("catalogue", &catalog),
    ] {
        if let Some(reason) = &diagnosis.blocking {
            blocked.push(format!("{name}: {reason}"));
        }
    }
    HeldDiagnosis {
        record,
        tenants,
        data_sources,
        catalog,
        blocked,
    }
}

fn document(
    kind: DocumentKind,
    held: &ObservedDocument,
    recorded: Option<&RecordedDocument>,
) -> DocumentDiagnosis {
    let manifest = match held.manifest.as_deref().map(|bytes| parse_manifest(bytes, kind)) {
        None => ManifestState::Absent,
        Some(Ok(manifest)) => ManifestState::Held {
            revision: manifest.revision(),
        },
        Some(Err(detail)) => ManifestState::Unreadable { detail },
    };
    let payload_sha256 = held.payload.as_deref().map(sha256_hex);
    let matches_record = recorded.map(|recorded| payload_sha256.as_deref() == Some(recorded.sha256.as_str()));
    DocumentDiagnosis {
        manifest,
        payload_sha256,
        recorded: recorded.cloned(),
        matches_record,
        blocking: blocking(kind, held),
    }
}
