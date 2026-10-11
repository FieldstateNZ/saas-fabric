//! Format and migration compatibility of the publication record across the
//! publisher rollback floor (D01-21).
//!
//! Three formats are in play. Contract 1 without `resources` is what #136
//! wrote, contract 1 with `resources` is what #138 wrote, and contract 2 is
//! what this release writes. The older two are spelled out as JSON here, key
//! for key, rather than produced by today's types. A fixture built by the
//! code under test would move whenever that code moved.
//!
//! An older binary is stood in for by
//! [`check_publisher_floor`](crate::rollback_floor::check_publisher_floor)
//! at contract 1. Every publisher since #136 runs that check, against its
//! own constant, before it reads anything else in the record.

use fabric_core::{BindingRevision, DataSourceId};
use serde_json::json;

use crate::rollback_floor::check_publisher_floor;
use crate::{
    plan_publication, CatalogDocument, DataSourceDocument, DocumentInput, DocumentKind, DocumentManifest,
    DocumentOutcome, DocumentRevision, HeldDocument, HeldDocuments, PublicationError, PublicationPlan,
    PublicationRecord, RecordedDocument, RuntimeSnapshot, TenantBindingDocument, CONTRACT_VERSION,
    MINIMUM_READER_CONTRACT,
};

const TENANTS_JSON: &str = include_str!("../../../examples/tenants.json");
const DATA_SOURCES_JSON: &str = include_str!("../../../examples/data-sources.json");
const CATALOG_JSON: &str = include_str!("../../../examples/catalog.json");

/// The contract every publisher from #136 to #138 was built at.
const CONTRACT_ONE: u32 = 1;

fn snapshot() -> RuntimeSnapshot {
    let tenants: Vec<TenantBindingDocument> = serde_json::from_str(TENANTS_JSON).unwrap();
    let data_sources: Vec<DataSourceDocument> = serde_json::from_str(DATA_SOURCES_JSON).unwrap();
    let catalog: CatalogDocument = serde_json::from_str(CATALOG_JSON).unwrap();
    RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(4), tenants),
        data_sources: DocumentInput::new(DocumentRevision::new(4), data_sources),
        catalog: DocumentInput::new(DocumentRevision::new(4), catalog),
        registry: None,
    }
}

/// A manifest as a contract-1 publisher wrote it.
fn contract_one_manifest(document: &str) -> DocumentManifest {
    serde_json::from_value(json!({ "contract_version": 1, "document": document, "revision": 4 })).unwrap()
}

/// What a contract-1 publisher left behind after publishing [`snapshot`]:
/// the same documents, contract-1 manifests, and `record` as the record.
fn held_by_contract_one(published: &PublicationPlan, record: &serde_json::Value) -> HeldDocuments {
    let held = |document: &str, bytes: &[u8]| HeldDocument {
        manifest: Some(contract_one_manifest(document)),
        payload: Some(bytes.to_vec()),
    };
    HeldDocuments {
        tenants: held("tenants", &published.tenants.bytes),
        data_sources: held("data-sources", &published.data_sources.bytes),
        catalog: held("catalog", &published.catalog.bytes),
        record: Some(serde_json::to_vec(record).unwrap()),
        registry: None,
    }
}

fn documents(published: &PublicationPlan) -> serde_json::Value {
    let named = |bytes: &[u8]| {
        let recorded = RecordedDocument::of(DocumentRevision::new(4), bytes);
        json!({ "revision": 4, "sha256": recorded.sha256 })
    };
    json!({
        "tenants": named(&published.tenants.bytes),
        "data-sources": named(&published.data_sources.bytes),
        "catalog": named(&published.catalog.bytes),
    })
}

/// The record #136 wrote: five keys, contract 1, both floors 1.
fn record_136(published: &PublicationPlan) -> serde_json::Value {
    json!({
        "contract_version": 1,
        "minimum_reader_contract": 1,
        "minimum_publisher_contract": 1,
        "generation": 6,
        "documents": documents(published),
    })
}

/// The record #138 wrote: #136's, plus resource history naming a DataSource
/// retired at revision 5 that no held document still holds.
fn record_138(published: &PublicationPlan) -> serde_json::Value {
    let mut record = record_136(published);
    record["resources"] = json!({ "tenants": {}, "data-sources": { "retired-postgres": 5 } });
    record
}

fn this_release() -> PublicationPlan {
    plan_publication(&snapshot(), &HeldDocuments::default()).unwrap()
}

#[test]
fn a_record_in_the_136_format_is_read_and_rewritten_at_this_contract_without_touching_a_document() {
    let published = this_release();
    let held = held_by_contract_one(&published, &record_136(&published));

    let next = plan_publication(&snapshot(), &held).unwrap();

    assert_eq!(next.tenants.outcome, DocumentOutcome::Unchanged);
    assert_eq!(next.data_sources.outcome, DocumentOutcome::Unchanged);
    assert_eq!(next.catalog.outcome, DocumentOutcome::Unchanged);
    assert_eq!(
        next.record.outcome,
        DocumentOutcome::Written,
        "the floor is raised by rewriting the record"
    );
    assert_eq!(next.record.record.generation(), 7);
    assert_eq!(next.record.record.minimum_publisher_contract(), CONTRACT_VERSION);
    assert_eq!(
        next.record.record.minimum_reader_contract(),
        MINIMUM_READER_CONTRACT
    );
    let tenant = &snapshot().tenants.payload[0];
    assert_eq!(
        next.record.record.resources().tenant(&tenant.tenant),
        Some(tenant.revision),
        "history starts from the held documents"
    );
}

#[test]
fn a_record_in_the_138_format_keeps_its_resource_history_across_the_floor_raise() {
    let published = this_release();
    let held = held_by_contract_one(&published, &record_138(&published));

    let next = plan_publication(&snapshot(), &held).unwrap();

    assert_eq!(next.record.record.minimum_publisher_contract(), CONTRACT_VERSION);
    assert_eq!(
        next.record
            .record
            .resources()
            .data_source(&DataSourceId::try_new("retired-postgres").unwrap()),
        Some(BindingRevision::new(5))
    );
}

#[test]
fn this_contract_is_above_the_one_the_136_to_138_publishers_were_built_at() {
    assert_eq!(
        CONTRACT_VERSION,
        CONTRACT_ONE + 1,
        "the release floor raised the publisher contract by one"
    );
    assert_eq!(
        MINIMUM_READER_CONTRACT, 1,
        "readers from #136 on still serve what this writes"
    );
}

#[test]
fn a_record_this_release_writes_is_refused_by_a_contract_one_publisher_before_its_history_is_read() {
    let written = this_release().record.bytes;

    let refused = check_publisher_floor(&written, CONTRACT_ONE).unwrap_err();

    assert!(
        matches!(
            refused,
            PublicationError::BelowRollbackFloor { floor, publisher }
                if floor == CONTRACT_VERSION && publisher == CONTRACT_ONE
        ),
        "{refused}"
    );
}

#[test]
fn a_record_this_release_writes_admits_this_release() {
    let written = this_release().record.bytes;

    check_publisher_floor(&written, CONTRACT_VERSION).unwrap();
    assert_eq!(
        PublicationRecord::parse_held(&written).unwrap(),
        this_release().record.record
    );
}

#[test]
fn a_record_from_a_later_floor_is_refused_by_this_release_whatever_else_it_carries() {
    let published = this_release();
    let mut later = record_138(&published);
    later["minimum_publisher_contract"] = json!(CONTRACT_VERSION + 1);
    later["contract_version"] = json!(CONTRACT_VERSION + 1);
    later["a_section_this_release_has_never_seen"] = json!({ "kept": true });
    let held = held_by_contract_one(&published, &later);

    let refused = plan_publication(&snapshot(), &held).unwrap_err();

    assert!(
        matches!(
            refused,
            PublicationError::BelowRollbackFloor { floor, publisher }
                if floor == CONTRACT_VERSION + 1 && publisher == CONTRACT_VERSION
        ),
        "{refused}"
    );
}

#[test]
fn a_contract_one_manifest_is_still_read_as_the_documents_revision() {
    let published = this_release();
    let held = held_by_contract_one(&published, &record_136(&published));

    assert_eq!(
        held.tenants.manifest.as_ref().unwrap().contract_version(),
        CONTRACT_ONE
    );
    assert_eq!(
        held.tenants.manifest.as_ref().unwrap().document(),
        DocumentKind::Tenants
    );
    assert_eq!(held.revisions().tenants, Some(DocumentRevision::new(4)));
}

/// [`snapshot`] carrying an issuer registry (D01-4) for its first tenant.
fn snapshot_with_registry() -> RuntimeSnapshot {
    let mut snapshot = snapshot();
    let tenant = snapshot.tenants.payload[0].tenant.clone();
    snapshot.registry = Some(vec![crate::IssuerRegistration {
        issuer: crate::RegistryIssuer::parse(&format!("https://identity.example.invalid/realms/{tenant}"))
            .unwrap(),
        tenant,
    }]);
    snapshot
}

#[test]
fn a_record_carrying_a_registry_keeps_this_contracts_floors_and_a_contract_one_publisher_refuses_it_by_its_floor(
) {
    let written = plan_publication(&snapshot_with_registry(), &HeldDocuments::default()).unwrap();
    let record: serde_json::Value = serde_json::from_slice(&written.record.bytes).unwrap();

    assert_eq!(record["contract_version"], CONTRACT_VERSION);
    assert_eq!(record["minimum_publisher_contract"], CONTRACT_VERSION);
    assert_eq!(record["minimum_reader_contract"], MINIMUM_READER_CONTRACT);
    assert_eq!(record["registry"]["generation"], 1);
    assert!(record["resources"].is_object(), "{record}");
    assert_eq!(
        PublicationRecord::parse_held(&written.record.bytes).unwrap(),
        written.record.record
    );
    assert!(
        matches!(
            check_publisher_floor(&written.record.bytes, CONTRACT_ONE),
            Err(PublicationError::BelowRollbackFloor { floor, .. }) if floor == CONTRACT_VERSION
        ),
        "the registry key never reaches a publisher below the floor"
    );
}

#[test]
fn a_registry_added_over_a_138_format_record_keeps_its_history_and_raises_the_floor() {
    let published = this_release();
    let held = held_by_contract_one(&published, &record_138(&published));

    let next = plan_publication(&snapshot_with_registry(), &held).unwrap();

    assert_eq!(next.record.record.minimum_publisher_contract(), CONTRACT_VERSION);
    assert_eq!(next.record.record.registry().unwrap().generation.get(), 1);
    assert_eq!(
        next.record
            .record
            .resources()
            .data_source(&DataSourceId::try_new("retired-postgres").unwrap()),
        Some(BindingRevision::new(5))
    );
}

#[test]
fn an_unchanged_republication_with_a_registry_rewrites_neither_the_registry_nor_the_record() {
    let first = plan_publication(&snapshot_with_registry(), &HeldDocuments::default()).unwrap();
    let held_document = |document: DocumentKind, plan: &crate::DocumentPlan| HeldDocument {
        manifest: Some(DocumentManifest::new(document, plan.revision)),
        payload: Some(plan.bytes.clone()),
    };
    let held = HeldDocuments {
        tenants: held_document(DocumentKind::Tenants, &first.tenants),
        data_sources: held_document(DocumentKind::DataSources, &first.data_sources),
        catalog: held_document(DocumentKind::Catalog, &first.catalog),
        record: Some(first.record.bytes.clone()),
        registry: first.registry.as_ref().map(|registry| registry.bytes.clone()),
    };

    let next = plan_publication(&snapshot_with_registry(), &held).unwrap();

    assert_eq!(next.record.outcome, DocumentOutcome::Unchanged);
    assert_eq!(next.registry.unwrap().outcome, DocumentOutcome::Unchanged);
}
