//! What is held, and every reason it is not clear, read without writing and
//! without the plan's first-refusal-wins order.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): the
//! report type and the one function that fills it, so every problem it can
//! raise reads in one place.

use super::held_parse::blocking;
use super::held_parts::{document, record_state, resource_problems};
use super::held_state::{DocumentDiagnosis, ManifestState, RecordState, RegistryDiagnosis};
use super::problem::{Part, Problem, ProblemKind};
use super::HeldObservation;
use crate::digest::sha256_hex;
use crate::{DocumentKind, IssuerRegistryDocument, ObservedDocument, PublicationRecord, RecordedDocument};

/// The held state of a publication target, as the recovery command
/// reports it. `clear` only when nothing is wrong with any recorded part.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HeldDiagnosis {
    /// Whether the record stayed the same while every part was read.
    pub stable: bool,
    /// The publication record.
    pub record: RecordState,
    /// `tenants.json` and its manifest.
    pub tenants: DocumentDiagnosis,
    /// `data-sources.json` and its manifest.
    pub data_sources: DocumentDiagnosis,
    /// `catalog.json` and its manifest.
    pub catalog: DocumentDiagnosis,
    /// `issuer-registry.json`.
    pub issuer_registry: RegistryDiagnosis,
    /// Every reason the held evidence is not clear.
    pub problems: Vec<Problem>,
    /// No problem at all. Says nothing about any offer a later publication
    /// makes, whose own guards are decided against it.
    pub clear: bool,
}

/// Diagnoses what an adapter observed. Pure; writes nothing.
#[must_use]
pub fn diagnose(held: &HeldObservation) -> HeldDiagnosis {
    let observation = &held.observation;
    let mut problems = Vec::new();
    let mut problem =
        |part: Part, kind: ProblemKind, detail: String| problems.push(Problem { part, kind, detail });
    if !held.stable {
        problem(
            Part::Observation,
            ProblemKind::Unstable,
            "the record changed while the target was read; inspect again".to_owned(),
        );
    }
    let (record, parsed) = record_state(observation.record.as_deref());
    if let RecordState::Unusable { detail, .. } = &record {
        problem(Part::Record, ProblemKind::BlocksPublication, detail.clone());
    }
    let documents = parsed.as_ref().map(PublicationRecord::documents);
    let mut diagnose_document =
        |kind: DocumentKind, held: &ObservedDocument, recorded: Option<&RecordedDocument>| {
            let diagnosis = document(kind, held, recorded);
            if let Some(reason) = blocking(kind, held) {
                problem(kind.into(), ProblemKind::BlocksPublication, reason);
            }
            if diagnosis.matches_record == Some(false) {
                problem(
                    kind.into(),
                    ProblemKind::Mismatch,
                    "the held payload is not the bytes the record names".to_owned(),
                );
            }
            if let (ManifestState::Held { revision }, Some(recorded)) = (&diagnosis.manifest, recorded) {
                if *revision != recorded.revision {
                    problem(
                        kind.into(),
                        ProblemKind::Mismatch,
                        format!(
                            "the manifest states revision {revision}, the record {}",
                            recorded.revision
                        ),
                    );
                }
            }
            diagnosis
        };
    let tenants = diagnose_document(
        DocumentKind::Tenants,
        &observation.tenants,
        documents.map(|d| &d.tenants),
    );
    let data_sources = diagnose_document(
        DocumentKind::DataSources,
        &observation.data_sources,
        documents.map(|d| &d.data_sources),
    );
    let catalog = diagnose_document(
        DocumentKind::Catalog,
        &observation.catalog,
        documents.map(|d| &d.catalog),
    );

    let recorded_registry = parsed.as_ref().and_then(PublicationRecord::registry);
    problems.extend(resource_problems(observation, recorded_registry));
    let issuer_registry = RegistryDiagnosis {
        recorded: recorded_registry.cloned(),
        held_sha256: observation.registry.as_deref().map(sha256_hex),
        held_generation: observation
            .registry
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<IssuerRegistryDocument>(bytes).ok())
            .map(|registry| registry.generation().into()),
    };
    let clear = problems.is_empty();
    HeldDiagnosis {
        stable: held.stable,
        record,
        tenants,
        data_sources,
        catalog,
        issuer_registry,
        problems,
        clear,
    }
}

#[cfg(test)]
#[path = "diagnosis_tests.rs"]
mod tests;
