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

    let revisions = held.revisions();

    assert_eq!(revisions.tenants, Some(DocumentRevision::new(3)));
    assert_eq!(revisions.catalog, None);
}
