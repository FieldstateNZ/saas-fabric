//! Reading each held part into what a diagnosis reports about it.

use super::held_parse::{parse_payload, read_manifest, record_refusal, ManifestReading, Parsed};
use super::held_state::{DocumentDiagnosis, ManifestState, RecordState};
use super::problem::{Part, Problem, ProblemKind};
use super::semantics::{dangling, data_source_problems, registry_problems, tenant_problems};
use crate::digest::sha256_hex;
use crate::{
    DataSourceDocument, DocumentKind, ObservedDocument, PublicationRecord, RecordedDocument,
    RecordedRegistry, TargetObservation, TenantBindingDocument,
};

/// Every resource rule the held documents and registry break, each checked
/// only where what it needs parses.
pub(super) fn resource_problems(
    observation: &TargetObservation,
    recorded_registry: Option<&RecordedRegistry>,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut problem =
        |part: Part, kind: ProblemKind, detail: String| problems.push(Problem { part, kind, detail });
    let held_tenants = parsed_tenants(observation);
    let held_data_sources = parsed_data_sources(observation);
    for detail in held_tenants.as_deref().map(tenant_problems).unwrap_or_default() {
        problem(Part::Tenants, ProblemKind::Unusable, detail);
    }
    for detail in held_data_sources
        .as_deref()
        .map(data_source_problems)
        .unwrap_or_default()
    {
        problem(Part::DataSources, ProblemKind::Unusable, detail);
    }
    if let (Some(tenants), Some(data_sources)) = (&held_tenants, &held_data_sources) {
        for detail in dangling(tenants, data_sources) {
            problem(Part::Tenants, ProblemKind::Unusable, detail);
        }
    }
    if let Some(recorded) = recorded_registry {
        for detail in registry_problems(observation.registry.as_deref(), recorded, held_tenants.as_deref()) {
            problem(Part::IssuerRegistry, ProblemKind::Unusable, detail);
        }
    }
    problems
}

pub(super) fn record_state(bytes: Option<&[u8]>) -> (RecordState, Option<PublicationRecord>) {
    let Some(bytes) = bytes else {
        return (RecordState::Absent, None);
    };
    let sha256 = sha256_hex(bytes);
    match PublicationRecord::parse_held(bytes) {
        Ok(record) => (
            RecordState::Held {
                generation: record.generation(),
                sha256,
            },
            Some(record),
        ),
        Err(error) => (
            RecordState::Unusable {
                detail: record_refusal(&error),
                sha256,
            },
            None,
        ),
    }
}

pub(super) fn parsed_tenants(observation: &TargetObservation) -> Option<Vec<TenantBindingDocument>> {
    match parse_payload(observation.tenants.payload.as_deref()?, DocumentKind::Tenants) {
        Ok(Parsed::Tenants(tenants)) => Some(tenants),
        _ => None,
    }
}

pub(super) fn parsed_data_sources(observation: &TargetObservation) -> Option<Vec<DataSourceDocument>> {
    match parse_payload(
        observation.data_sources.payload.as_deref()?,
        DocumentKind::DataSources,
    ) {
        Ok(Parsed::DataSources(data_sources)) => Some(data_sources),
        _ => None,
    }
}

pub(super) fn document(
    kind: DocumentKind,
    held: &ObservedDocument,
    recorded: Option<&RecordedDocument>,
) -> DocumentDiagnosis {
    let manifest = match read_manifest(held.manifest.as_deref(), kind) {
        ManifestReading::Missing => ManifestState::Absent,
        ManifestReading::Valid(manifest) => ManifestState::Held {
            revision: manifest.revision(),
        },
        ManifestReading::Newer(manifest) => ManifestState::Newer {
            revision: manifest.revision(),
            contract_version: manifest.contract_version(),
        },
        ManifestReading::Malformed(detail) => ManifestState::Malformed { detail },
        ManifestReading::Conflicting(detail) => ManifestState::Conflicting { detail },
    };
    let payload_sha256 = held.payload.as_deref().map(sha256_hex);
    let matches_record = recorded.map(|recorded| payload_sha256.as_deref() == Some(recorded.sha256.as_str()));
    DocumentDiagnosis {
        manifest,
        payload_sha256,
        recorded: recorded.cloned(),
        matches_record,
    }
}
