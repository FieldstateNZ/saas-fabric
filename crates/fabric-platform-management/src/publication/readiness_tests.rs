use std::collections::BTreeSet;

use fabric_core::TenantId;
use fabric_runtime_publication::{
    CompletenessGap, DocumentRevision, GapKind, PublicationCompleteness, PublishedRevisions, RecordStamp,
    RecordedPart, RegistryGeneration, UnobservedReason,
};

use super::*;
use crate::publication::completeness_observation::CompletenessObservation;
use crate::publication::exclusion::Exclusion;
use crate::publication::readiness_evidence::{ReadinessEvidence, RegistryEvidenceSummary};

fn tenant(name: &str) -> TenantId {
    TenantId::try_new(name).unwrap()
}

fn published() -> PublishedRevisions {
    PublishedRevisions {
        tenants: Some(DocumentRevision::new(2)),
        data_sources: Some(DocumentRevision::new(1)),
        catalog: Some(DocumentRevision::new(1)),
        registry: Some(RegistryGeneration::FIRST),
    }
}

fn evidence(registry: Option<RegistryEvidenceSummary>) -> ReadinessEvidence {
    ReadinessEvidence {
        revisions: published(),
        registry,
        connectors: BTreeSet::from(["postgres-alpha".to_owned()]),
        record: fabric_runtime_publication::RecordStamp {
            generation: 3,
            sha256: fabric_core::RecordDigest::from_bytes([3; 32]),
        },
    }
}

fn stamp() -> RecordStamp {
    RecordStamp {
        generation: 3,
        sha256: fabric_core::RecordDigest::from_bytes([3; 32]),
    }
}

fn complete() -> PublicationCompleteness {
    PublicationCompleteness::Complete { record: stamp() }
}

fn observed(completeness: PublicationCompleteness) -> CompletenessObservation {
    CompletenessObservation {
        at_unix_seconds: 1_700_000_000,
        completeness,
    }
}

fn registering(tenants: &[&str]) -> RegistryEvidenceSummary {
    RegistryEvidenceSummary {
        generation: RegistryGeneration::FIRST,
        registered: tenants.iter().copied().map(tenant).collect(),
        excluded: vec![Excluded {
            tenant: tenant("beta"),
            reason: Exclusion::NotReady {
                unmet: vec!["identity: identity has not been applied at this revision yet".to_owned()],
            },
        }],
    }
}

fn reasons(status: &GateStatus) -> Vec<String> {
    match status {
        GateStatus::Met => Vec::new(),
        GateStatus::Unmet { reasons } => reasons.clone(),
    }
}

#[test]
fn without_an_accepted_pass_every_gate_is_unmet() {
    let report = ReadinessReport::of(None, Some(&observed(complete())));

    for gate in [&report.documents, &report.identity, &report.connectors] {
        assert!(matches!(gate, GateStatus::Unmet { .. }), "{gate:?}");
    }
}

#[test]
fn published_documents_and_a_registry_naming_an_issuer_meet_two_gates_and_name_the_excluded() {
    let report = ReadinessReport::of(
        Some(&evidence(Some(registering(&["alpha"])))),
        Some(&observed(complete())),
    );

    assert_eq!(report.documents, GateStatus::Met);
    assert_eq!(report.identity, GateStatus::Met);
    assert_eq!(report.excluded.len(), 1);
    assert_eq!(report.excluded[0].tenant, tenant("beta"));
    assert_eq!(report.excluded[0].reason.kind(), "notReady");
    assert_eq!(
        report.application,
        RegistryApplication::Unobserved,
        "a published registry is not evidence that anything applied it"
    );
}

#[test]
fn the_connectors_gate_is_never_met_by_fabric_and_names_what_composition_must_supply() {
    let report = ReadinessReport::of(
        Some(&evidence(Some(registering(&["alpha"])))),
        Some(&observed(complete())),
    );

    let reasons = reasons(&report.connectors);
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].contains("postgres-alpha"), "{reasons:?}");
}

#[test]
fn no_registry_or_one_naming_no_issuer_leaves_the_identity_gate_unmet() {
    let none = ReadinessReport::of(Some(&evidence(None)), Some(&observed(complete())));
    let empty = ReadinessReport::of(
        Some(&evidence(Some(registering(&[])))),
        Some(&observed(complete())),
    );

    assert!(reasons(&none.identity)[0].contains("no issuer registry"));
    assert!(reasons(&empty.identity)[0].contains("registers no issuer"));
}

#[test]
fn a_document_never_published_leaves_the_documents_gate_unmet_naming_it() {
    let mut partial = evidence(Some(registering(&["alpha"])));
    partial.revisions.catalog = None;

    let report = ReadinessReport::of(Some(&partial), Some(&observed(complete())));

    assert_eq!(
        reasons(&report.documents),
        vec!["catalog is not published".to_owned()]
    );
}

#[test]
fn the_documents_gate_needs_the_target_observed_whole_and_says_why_it_is_not() {
    let evidence = evidence(Some(registering(&["alpha"])));
    let incomplete = PublicationCompleteness::Incomplete {
        record: stamp(),
        gaps: vec![
            CompletenessGap {
                part: RecordedPart::DataSources,
                kind: GapKind::DigestMismatch,
            },
            CompletenessGap {
                part: RecordedPart::Tenants,
                kind: GapKind::ManifestMissing,
            },
        ],
    };
    let cases = [
        (
            Some(observed(incomplete)),
            "the target does not hold generation 3 whole: data-sources payload does not match the \
             record's digest; tenants manifest is missing",
        ),
        (
            Some(observed(PublicationCompleteness::Unpublished)),
            "the target holds no publication record",
        ),
        (
            Some(observed(PublicationCompleteness::Unobserved {
                reason: UnobservedReason::Unstable,
            })),
            "the target's completeness is unobserved: the publication record changed while the target was read",
        ),
        (
            None,
            "no publication pass has inspected the target since the control plane started",
        ),
    ];

    for (observation, reason) in cases {
        let report = ReadinessReport::of(Some(&evidence), observation.as_ref());
        assert_eq!(reasons(&report.documents), vec![reason.to_owned()]);
        assert_eq!(
            report.identity,
            GateStatus::Met,
            "only the documents gate reads completeness"
        );
    }
}

#[test]
fn a_complete_target_does_not_stand_in_for_an_accepted_pass() {
    let report = ReadinessReport::of(None, Some(&observed(complete())));

    assert_eq!(
        reasons(&report.documents),
        vec!["no publication pass has been accepted since the control plane started".to_owned()]
    );
}
