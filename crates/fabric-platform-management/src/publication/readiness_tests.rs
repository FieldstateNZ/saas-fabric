use std::collections::BTreeSet;

use fabric_core::TenantId;
use fabric_runtime_publication::{DocumentRevision, PublishedRevisions, RegistryGeneration};

use super::*;
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
    let report = ReadinessReport::of(None);

    for gate in [&report.documents, &report.identity, &report.connectors] {
        assert!(matches!(gate, GateStatus::Unmet { .. }), "{gate:?}");
    }
}

#[test]
fn published_documents_and_a_registry_naming_an_issuer_meet_two_gates_and_name_the_excluded() {
    let report = ReadinessReport::of(Some(&evidence(Some(registering(&["alpha"])))));

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
    let report = ReadinessReport::of(Some(&evidence(Some(registering(&["alpha"])))));

    let reasons = reasons(&report.connectors);
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].contains("postgres-alpha"), "{reasons:?}");
}

#[test]
fn no_registry_or_one_naming_no_issuer_leaves_the_identity_gate_unmet() {
    let none = ReadinessReport::of(Some(&evidence(None)));
    let empty = ReadinessReport::of(Some(&evidence(Some(registering(&[])))));

    assert!(reasons(&none.identity)[0].contains("no issuer registry"));
    assert!(reasons(&empty.identity)[0].contains("registers no issuer"));
}

#[test]
fn a_document_never_published_leaves_the_documents_gate_unmet_naming_it() {
    let mut partial = evidence(Some(registering(&["alpha"])));
    partial.revisions.catalog = None;

    let report = ReadinessReport::of(Some(&partial));

    assert_eq!(
        reasons(&report.documents),
        vec!["catalog is not published".to_owned()]
    );
}
