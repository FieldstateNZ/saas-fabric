//! The plan decides every refusal the port promises, without touching I/O.

use super::*;
use crate::{
    CatalogDocument, ConnectionSelectorDocument, DataResidencyDocument, DataSourceDocument, DocumentInput,
    DocumentManifest, DocumentRevision, Emptying, PlacementClassDocument, ResourceDefinitionDocument,
    TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings,
};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, TenantId};
use std::collections::BTreeMap;

fn data_source(id: &str) -> DataSourceDocument {
    DataSourceDocument {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(1),
        connector: crate::ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: crate::ConnectionName::try_new("shared").unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: crate::PoolSettingsDocument::default(),
        capabilities: crate::DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    }
}

fn tenant(id: &str, data_source: &str) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new(data_source).unwrap(),
            isolation: crate::IsolationModelDocument::Database {},
        },
    );
    TenantBindingDocument {
        tenant: TenantId::try_new(id).unwrap(),
        revision: BindingRevision::new(1),
        data: TenantDataBindings::try_new(data).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    }
}

fn catalog() -> CatalogDocument {
    let mut resources = BTreeMap::new();
    resources.insert(
        LogicalResourceName::try_new("customers").unwrap(),
        ResourceDefinitionDocument {
            data_source: LogicalDataSourceName::try_new("primary").unwrap(),
            collection: crate::CollectionName::try_new("customers").unwrap(),
            key_field: crate::FieldName::try_new("id").unwrap(),
            operations: vec![fabric_core::OperationKind::Read],
            queryable_fields: vec![],
        },
    );
    CatalogDocument::new(resources)
}

fn snapshot(revision: u64) -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(revision), vec![tenant("acme", "shared-1")]),
        data_sources: DocumentInput::new(DocumentRevision::new(revision), vec![data_source("shared-1")]),
        catalog: DocumentInput::new(DocumentRevision::new(revision), catalog()),
        registry: None,
    }
}

fn held_from(plan: &PublicationPlan) -> HeldDocuments {
    let held = |kind: DocumentKind, document: &DocumentPlan| HeldDocument {
        manifest: Some(DocumentManifest::new(kind, document.revision)),
        payload: Some(document.bytes.clone()),
    };
    HeldDocuments {
        tenants: held(DocumentKind::Tenants, &plan.tenants),
        data_sources: held(DocumentKind::DataSources, &plan.data_sources),
        catalog: held(DocumentKind::Catalog, &plan.catalog),
        record: Some(plan.record.bytes.clone()),
        registry: None,
    }
}

#[test]
fn a_first_publication_writes_all_three_documents() {
    let plan = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();

    assert_eq!(plan.data_sources.outcome, DocumentOutcome::Written);
    assert_eq!(plan.catalog.outcome, DocumentOutcome::Written);
    assert_eq!(plan.tenants.outcome, DocumentOutcome::Written);
    assert_eq!(plan.tenants.revision, DocumentRevision::new(1));
}

#[test]
fn the_same_snapshot_at_the_held_revision_changes_nothing() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();

    let again = plan_publication(&snapshot(1), &held_from(&first)).unwrap();

    assert_eq!(again.data_sources.outcome, DocumentOutcome::Unchanged);
    assert_eq!(again.catalog.outcome, DocumentOutcome::Unchanged);
    assert_eq!(again.tenants.outcome, DocumentOutcome::Unchanged);
}

#[test]
fn an_older_revision_than_the_one_held_is_stale() {
    let first = plan_publication(&snapshot(2), &HeldDocuments::default()).unwrap();

    let error = plan_publication(&snapshot(1), &held_from(&first)).unwrap_err();

    assert!(matches!(error, PublicationError::StaleRevision { .. }), "{error}");
}

#[test]
fn different_bytes_at_the_held_revision_diverge() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();
    let mut changed = snapshot(1);
    changed.tenants.payload.push(tenant("globex", "shared-1"));

    let error = plan_publication(&changed, &held_from(&first)).unwrap_err();

    assert!(
        matches!(
            error,
            PublicationError::DivergentPayload {
                document: DocumentKind::Tenants,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn a_tenant_naming_a_data_source_the_snapshot_lacks_is_dangling() {
    let mut offered = snapshot(1);
    offered.data_sources.payload.clear();
    offered.data_sources = offered.data_sources.emptying_intended();

    let error = plan_publication(&offered, &HeldDocuments::default()).unwrap_err();

    assert!(
        matches!(error, PublicationError::DanglingDataSource { .. }),
        "{error}"
    );
}

#[test]
fn emptying_a_held_document_needs_stated_intent() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();
    let mut emptied = snapshot(2);
    emptied.tenants.payload.clear();

    let error = plan_publication(&emptied, &held_from(&first)).unwrap_err();
    assert!(
        matches!(error, PublicationError::EmptyingNotIntended { .. }),
        "{error}"
    );

    let mut stated = snapshot(2);
    stated.tenants.payload.clear();
    stated.tenants.emptying = Emptying::Intended;
    let plan = plan_publication(&stated, &held_from(&first)).unwrap();
    assert_eq!(plan.tenants.outcome, DocumentOutcome::Written);
}

#[test]
fn a_manifest_whose_payload_is_gone_is_refused() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();
    let mut held = held_from(&first);
    held.tenants.payload = None;

    let error = plan_publication(&snapshot(2), &held).unwrap_err();

    assert!(
        matches!(
            error,
            PublicationError::HeldPayloadLost {
                document: DocumentKind::Tenants
            }
        ),
        "{error}"
    );
}

#[test]
fn revisions_report_none_for_a_document_never_published() {
    let first = plan_publication(&snapshot(3), &HeldDocuments::default()).unwrap();
    let mut held = held_from(&first);
    held.catalog = HeldDocument::absent();
    held.record = None;

    let revisions = held.revisions();

    assert_eq!(revisions.tenants, Some(DocumentRevision::new(3)));
    assert_eq!(revisions.catalog, None);
}

#[test]
fn a_first_publication_writes_a_record_at_generation_one_naming_every_document() {
    let plan = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();

    assert_eq!(plan.record.outcome, DocumentOutcome::Written);
    assert_eq!(plan.record.record.generation(), 1);
    let documents = plan.record.record.documents();
    assert_eq!(
        documents.tenants,
        crate::RecordedDocument::of(plan.tenants.revision, &plan.tenants.bytes)
    );
    assert_eq!(
        documents.data_sources,
        crate::RecordedDocument::of(plan.data_sources.revision, &plan.data_sources.bytes)
    );
    assert_eq!(
        documents.catalog,
        crate::RecordedDocument::of(plan.catalog.revision, &plan.catalog.bytes)
    );
}

#[test]
fn republishing_the_held_generation_leaves_the_record_and_its_generation_alone() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();

    let again = plan_publication(&snapshot(1), &held_from(&first)).unwrap();

    assert_eq!(again.record.outcome, DocumentOutcome::Unchanged);
    assert_eq!(again.record.bytes, first.record.bytes);
}

#[test]
fn a_change_to_any_one_document_advances_the_generation() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();
    let mut catalogue_only = snapshot(1);
    catalogue_only.catalog = DocumentInput::new(DocumentRevision::new(2), catalog());

    let next = plan_publication(&catalogue_only, &held_from(&first)).unwrap();

    assert_eq!(next.tenants.outcome, DocumentOutcome::Unchanged);
    assert_eq!(next.data_sources.outcome, DocumentOutcome::Unchanged);
    assert_eq!(next.record.outcome, DocumentOutcome::Written);
    assert_eq!(next.record.record.generation(), 2);
    assert_eq!(
        next.record.record.documents().catalog.revision,
        DocumentRevision::new(2)
    );
}

#[test]
fn a_set_published_before_the_record_existed_gains_one_without_rewriting_a_document() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();
    let mut unrecorded = held_from(&first);
    unrecorded.record = None;

    let next = plan_publication(&snapshot(1), &unrecorded).unwrap();

    assert_eq!(next.tenants.outcome, DocumentOutcome::Unchanged);
    assert_eq!(next.data_sources.outcome, DocumentOutcome::Unchanged);
    assert_eq!(next.catalog.outcome, DocumentOutcome::Unchanged);
    assert_eq!(next.record.outcome, DocumentOutcome::Written);
    assert_eq!(next.record.record.generation(), 1);
}

#[test]
fn a_held_record_above_this_publishers_rollback_floor_refuses_the_whole_publication() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();
    let mut held = held_from(&first);
    let mut future: serde_json::Value = serde_json::from_slice(&first.record.bytes).unwrap();
    future["minimum_publisher_contract"] = serde_json::json!(crate::CONTRACT_VERSION + 1);
    future["a_field_this_publisher_has_never_seen"] = serde_json::json!(true);
    held.record = Some(serde_json::to_vec(&future).unwrap());

    let refused = plan_publication(&snapshot(2), &held).unwrap_err();

    assert!(
        matches!(
            refused,
            PublicationError::BelowRollbackFloor { floor, publisher }
                if floor == crate::CONTRACT_VERSION + 1 && publisher == crate::CONTRACT_VERSION
        ),
        "{refused}"
    );
}

#[test]
fn a_held_record_that_does_not_parse_is_unreadable_as_the_tenants_document() {
    let first = plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap();
    let mut held = held_from(&first);
    held.record = Some(b"{ torn".to_vec());

    let refused = plan_publication(&snapshot(2), &held).unwrap_err();

    assert!(
        matches!(
            refused,
            PublicationError::Unreadable {
                document: DocumentKind::Tenants,
                ..
            }
        ),
        "{refused}"
    );
}

#[test]
fn a_held_tenant_offered_at_an_older_revision_is_refused() {
    let mut newer = snapshot(1);
    newer.tenants.payload[0].revision = BindingRevision::new(5);
    let held = held_from(&plan_publication(&newer, &HeldDocuments::default()).unwrap());
    let mut lowered = snapshot(2);
    lowered.tenants.payload[0].revision = BindingRevision::new(4);

    let refused = plan_publication(&lowered, &held).unwrap_err();

    assert!(
        matches!(
            &refused,
            PublicationError::ResourceRevisionNotAdvanced { document: DocumentKind::Tenants, resource, held, offered }
                if resource == "acme" && *held == BindingRevision::new(5) && *offered == BindingRevision::new(4)
        ),
        "{refused}"
    );
}

#[test]
fn a_held_data_source_changed_at_the_same_revision_is_refused() {
    let held = held_from(&plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap());
    let mut changed = snapshot(2);
    changed.data_sources.payload[0].residency.region = "au".to_owned();

    let refused = plan_publication(&changed, &held).unwrap_err();

    assert!(
        matches!(
            &refused,
            PublicationError::ResourceRevisionNotAdvanced { document: DocumentKind::DataSources, resource, .. }
                if resource == "shared-1"
        ),
        "{refused}"
    );
}

#[test]
fn a_held_resource_changed_at_a_newer_revision_or_unchanged_at_the_same_one_is_accepted() {
    let held = held_from(&plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap());
    let mut changed = snapshot(2);
    changed.data_sources.payload[0].residency.region = "au".to_owned();
    changed.data_sources.payload[0].revision = BindingRevision::new(2);

    let plan = plan_publication(&changed, &held).unwrap();

    assert_eq!(plan.data_sources.outcome, DocumentOutcome::Written);
    assert_eq!(plan.tenants.outcome, DocumentOutcome::Written);
}

#[test]
fn a_resource_the_held_document_does_not_hold_may_start_at_any_revision() {
    let held = held_from(&plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap());
    let mut added = snapshot(2);
    added.data_sources.payload.push(data_source("shared-2"));

    assert!(plan_publication(&added, &held).is_ok());
}

#[test]
fn a_lost_manifest_is_held_at_the_revision_the_record_names_so_an_older_offer_is_refused() {
    let mut at_five = snapshot(5);
    at_five.tenants.payload[0].revision = BindingRevision::new(5);
    let mut held = held_from(&plan_publication(&at_five, &HeldDocuments::default()).unwrap());
    held.tenants.manifest = None;

    assert_eq!(held.revisions().tenants, Some(DocumentRevision::new(5)));
    let mut older_tenants = at_five.clone();
    older_tenants.tenants.revision = DocumentRevision::new(1);
    let refused = plan_publication(&older_tenants, &held).unwrap_err();

    assert!(
        matches!(
            refused,
            PublicationError::StaleRevision {
                document: DocumentKind::Tenants,
                ..
            }
        ),
        "{refused}"
    );
}

fn with_unbound_data_source(revision: u64, data_source_revision: u64) -> RuntimeSnapshot {
    let mut offered = snapshot(revision);
    let mut spare = data_source("shared-2");
    spare.revision = BindingRevision::new(data_source_revision);
    offered.data_sources.payload.push(spare);
    offered
}

#[test]
fn a_data_source_retired_and_declared_again_at_or_below_its_last_revision_is_refused() {
    let declared = plan_publication(&with_unbound_data_source(1, 3), &HeldDocuments::default()).unwrap();
    let retired = plan_publication(&snapshot(2), &held_from(&declared)).unwrap();
    assert_eq!(
        retired
            .record
            .record
            .resources()
            .data_source(&DataSourceId::try_new("shared-2").unwrap()),
        Some(BindingRevision::new(3))
    );

    for again in [1, 3] {
        let refused =
            plan_publication(&with_unbound_data_source(3, again), &held_from(&retired)).unwrap_err();
        assert!(
            matches!(
                &refused,
                PublicationError::ResourceRevisionNotAdvanced { document: DocumentKind::DataSources, resource, held, offered }
                    if resource == "shared-2" && *held == BindingRevision::new(3) && *offered == BindingRevision::new(again)
            ),
            "{refused}"
        );
    }
    assert!(plan_publication(&with_unbound_data_source(3, 4), &held_from(&retired)).is_ok());
}

#[test]
fn a_record_from_before_resource_history_starts_it_from_the_held_documents() {
    let declared = plan_publication(&with_unbound_data_source(1, 3), &HeldDocuments::default()).unwrap();
    let mut held = held_from(&declared);
    let mut before_history: serde_json::Value = serde_json::from_slice(&declared.record.bytes).unwrap();
    before_history.as_object_mut().unwrap().remove("resources");
    held.record = Some(serde_json::to_vec(&before_history).unwrap());

    let retired = plan_publication(&snapshot(2), &held).unwrap();

    let refused = plan_publication(&with_unbound_data_source(3, 1), &held_from(&retired)).unwrap_err();
    assert!(
        matches!(refused, PublicationError::ResourceRevisionNotAdvanced { .. }),
        "{refused}"
    );
}

#[test]
fn a_held_tenant_dropped_from_the_offer_is_refused() {
    let mut two = snapshot(1);
    two.tenants.payload.push(tenant("globex", "shared-1"));
    let held = held_from(&plan_publication(&two, &HeldDocuments::default()).unwrap());

    let refused = plan_publication(&snapshot(2), &held).unwrap_err();

    assert!(
        matches!(
            &refused,
            PublicationError::TenantRemoved { tenant, held }
                if tenant.as_str() == "globex" && *held == BindingRevision::new(1)
        ),
        "{refused}"
    );
}

#[test]
fn a_tenant_emptied_with_intent_and_placed_again_below_its_last_revision_is_refused() {
    let mut at_five = snapshot(1);
    at_five.tenants.payload[0].revision = BindingRevision::new(5);
    let published = plan_publication(&at_five, &HeldDocuments::default()).unwrap();
    let mut emptied = snapshot(2);
    emptied.tenants = DocumentInput::new(DocumentRevision::new(2), vec![]).emptying_intended();
    let emptied = plan_publication(&emptied, &held_from(&published)).unwrap();

    let refused = plan_publication(&snapshot(3), &held_from(&emptied)).unwrap_err();

    assert!(
        matches!(
            &refused,
            PublicationError::ResourceRevisionNotAdvanced { document: DocumentKind::Tenants, held, .. }
                if *held == BindingRevision::new(5)
        ),
        "{refused}"
    );
}

#[test]
fn a_catalogue_entry_on_a_logical_data_source_no_tenant_binds_is_refused() {
    let mut offered = snapshot(1);
    let mut resources: BTreeMap<_, _> = catalog()
        .resources()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut audit = resources.values().next().unwrap().clone();
    audit.data_source = LogicalDataSourceName::try_new("audit").unwrap();
    resources.insert(LogicalResourceName::try_new("auditEvents").unwrap(), audit);
    offered.catalog = DocumentInput::new(DocumentRevision::new(1), CatalogDocument::new(resources));

    let refused = plan_publication(&offered, &HeldDocuments::default()).unwrap_err();
    assert!(
        matches!(
            &refused,
            PublicationError::CatalogueEntryUnbound { resource, data_source }
                if resource.as_str() == "auditEvents" && data_source.as_str() == "audit"
        ),
        "{refused}"
    );

    offered.tenants.payload.clear();
    assert!(
        plan_publication(&offered, &HeldDocuments::default()).is_ok(),
        "a generation with no tenant serves no entry"
    );
}
