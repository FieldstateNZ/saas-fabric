//! The whole-snapshot refusal rules, tested without a filesystem: the
//! emptying guard, the empty-catalogue refusal, and the two referential
//! checks.

use std::collections::BTreeMap;

use fabric_core::{DataSourceId, LogicalDataSourceName, LogicalResourceName, TenantId};

use crate::validate::validate_snapshot;
use crate::{
    CatalogDocument, ConnectionSelectorDocument, DataResidencyDocument, DataSourceDocument, DocumentInput,
    DocumentRevision, IsolationModelDocument, PlacementClassDocument, PublicationError,
    ResourceDefinitionDocument, RuntimeSnapshot, TenantBindingDocument, TenantDataBindingDocument,
    TenantDataBindings,
};

fn data_source(id: &str) -> DataSourceDocument {
    DataSourceDocument {
        id: DataSourceId::try_new(id).unwrap(),
        revision: fabric_core::BindingRevision::new(1),
        connector: crate::ConnectorId::try_new("postgres-au-east").unwrap(),
        connection: ConnectionSelectorDocument::Default {},
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "au-east".to_owned(),
            jurisdiction: None,
        },
        pool: crate::PoolSettingsDocument::default(),
        capabilities: crate::DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    }
}

fn tenant(name: &str, bound_to: &str) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new(bound_to).unwrap(),
            isolation: IsolationModelDocument::Database {},
        },
    );

    TenantBindingDocument {
        tenant: TenantId::try_new(name).unwrap(),
        revision: fabric_core::BindingRevision::new(1),
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
            queryable_fields: Vec::new(),
        },
    );
    CatalogDocument::new(resources)
}

fn snapshot(tenants: Vec<TenantBindingDocument>, data_sources: Vec<DataSourceDocument>) -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(1), tenants),
        data_sources: DocumentInput::new(DocumentRevision::new(1), data_sources),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalog()),
    }
}

#[test]
fn an_empty_catalogue_is_refused_whatever_the_emptying_intent_says() {
    let mut offered = snapshot(vec![], vec![]);
    offered.catalog = DocumentInput::new(DocumentRevision::new(1), CatalogDocument::new(BTreeMap::new()))
        .emptying_intended();

    let error = validate_snapshot(&offered, &[], &[]).unwrap_err();

    assert!(matches!(error, PublicationError::EmptyCatalogue));
}

#[test]
fn taking_tenants_from_non_empty_to_empty_without_intent_is_refused() {
    let held_tenants = vec![tenant("acme", "sql-01")];
    let offered = snapshot(vec![], vec![data_source("sql-01")]);

    let error = validate_snapshot(&offered, &held_tenants, &[]).unwrap_err();

    assert!(matches!(
        error,
        PublicationError::EmptyingNotIntended {
            document: crate::DocumentKind::Tenants
        }
    ));
}

#[test]
fn taking_tenants_from_non_empty_to_empty_with_intent_is_allowed() {
    let held_tenants = vec![tenant("acme", "sql-01")];
    let mut offered = snapshot(vec![], vec![data_source("sql-01")]);
    offered.tenants = DocumentInput::new(DocumentRevision::new(1), vec![]).emptying_intended();

    assert!(validate_snapshot(&offered, &held_tenants, &[]).is_ok());
}

#[test]
fn an_already_empty_document_staying_empty_is_not_an_emptying() {
    // No held tenants at all -- an empty offering is not a *change*, so no
    // intent is required.
    let offered = snapshot(vec![], vec![]);

    assert!(validate_snapshot(&offered, &[], &[]).is_ok());
}

#[test]
fn a_tenant_naming_a_data_source_this_publication_does_not_include_is_refused() {
    let offered = snapshot(vec![tenant("acme", "sql-01")], vec![]);

    let error = validate_snapshot(&offered, &[], &[]).unwrap_err();

    assert!(matches!(error, PublicationError::DanglingDataSource { .. }));
}

#[test]
fn dropping_a_data_source_the_held_tenants_still_bind_is_refused() {
    let held_tenants = vec![tenant("acme", "sql-01")];
    let offered = snapshot(vec![], vec![]);

    let error = validate_snapshot(&offered, &held_tenants, &[]).unwrap_err();

    assert!(matches!(
        error,
        PublicationError::RetiredDataSourceStillBound { .. }
    ));
}

#[test]
fn an_absent_held_tenants_document_imposes_no_retirement_constraint() {
    // No prior publication ever happened -- nothing can be "still bound".
    let offered = snapshot(vec![], vec![]);

    assert!(validate_snapshot(&offered, &[], &[]).is_ok());
}
