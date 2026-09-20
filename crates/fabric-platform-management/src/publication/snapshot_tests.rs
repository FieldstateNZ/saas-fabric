//! `compose` is pure, so every ADR 0023 part 4 (D1/D2) rule is provable
//! without a repository, a target, or a catalogue source.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    data_sources_canonical_json, CatalogDocument, ConnectionName, ConnectionSelectorDocument, ConnectorId,
    DataResidencyDocument, DataSourceCapabilitiesDocument, DocumentRevision, FieldName,
    IsolationModelDocument, PlacementClassDocument, PoolSettingsDocument, PublishedRevisions,
};

use super::{compose, ComposeError};
use crate::placements::PlacementRecord;
use crate::{DataSourceDeclaration, Discriminator};

fn declaration(
    id: &str,
    placement: PlacementClassDocument,
    discriminator: Option<Discriminator>,
) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).expect("a valid data source id"),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres-nz").expect("a valid connector id"),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").expect("a valid connection name"),
        },
        placement,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        discriminator,
        labels: BTreeMap::new(),
    }
}

fn placement(tenant: &str, logical: &str, data_source: &str, revision: u64) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).expect("a valid tenant id"),
        logical: LogicalDataSourceName::try_new(logical).expect("a valid logical data source name"),
        revision: BindingRevision::new(revision),
        data_source: DataSourceId::try_new(data_source).expect("a valid data source id"),
        isolation: IsolationModelDocument::Database {},
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    }
}

fn empty_catalog() -> CatalogDocument {
    CatalogDocument::new(BTreeMap::new())
}

fn no_history() -> PublishedRevisions {
    PublishedRevisions::default()
}

#[test]
fn data_sources_are_sorted_by_id_even_when_input_is_not() {
    let declarations = vec![
        declaration("shared-b", PlacementClassDocument::Dedicated, None),
        declaration("shared-a", PlacementClassDocument::Dedicated, None),
    ];

    let snapshot = compose(declarations, &[], empty_catalog(), &no_history()).expect("no duplicates");

    let ids: Vec<&str> = snapshot
        .data_sources
        .payload
        .iter()
        .map(|document| document.id.as_str())
        .collect();
    assert_eq!(ids, vec!["shared-a", "shared-b"]);
}

#[test]
fn each_tenant_gets_one_binding_with_all_its_logical_entries() {
    let placements = vec![
        placement("acme", "primary", "shared-a", 1),
        placement("acme", "audit", "shared-b", 1),
    ];

    let snapshot = compose(vec![], &placements, empty_catalog(), &no_history()).expect("no duplicates");

    assert_eq!(snapshot.tenants.payload.len(), 1);
    let acme = &snapshot.tenants.payload[0];
    assert_eq!(acme.tenant.as_str(), "acme");
    assert_eq!(acme.data.len(), 2);
}

#[test]
fn a_tenants_revision_is_the_sum_of_its_records_own_revisions() {
    let placements = vec![
        placement("acme", "primary", "shared-a", 1),
        placement("acme", "audit", "shared-b", 1),
    ];

    let snapshot = compose(vec![], &placements, empty_catalog(), &no_history()).expect("no duplicates");

    assert_eq!(snapshot.tenants.payload[0].revision, BindingRevision::new(2));
}

/// The mutation target: summing rather than taking a maximum is what moves
/// a tenant's revision when a second record is added, and taking `max`
/// instead makes this assertion fail (`acme` would stay at 1).
#[test]
fn adding_a_record_moves_the_sum_and_leaves_another_tenant_unaffected() {
    let placements = vec![
        placement("acme", "primary", "shared-a", 1),
        placement("acme", "audit", "shared-b", 1),
        placement("globex", "primary", "shared-a", 5),
    ];

    let snapshot = compose(vec![], &placements, empty_catalog(), &no_history()).expect("no duplicates");

    let revision_of = |tenant: &str| {
        snapshot
            .tenants
            .payload
            .iter()
            .find(|binding| binding.tenant.as_str() == tenant)
            .expect("the tenant is in the snapshot")
            .revision
    };

    assert_eq!(revision_of("acme"), BindingRevision::new(2));
    assert_eq!(
        revision_of("globex"),
        BindingRevision::new(5),
        "globex has one record and is unaffected"
    );
}

#[test]
fn two_records_for_one_tenants_logical_data_source_are_refused() {
    let placements = vec![
        placement("acme", "primary", "shared-a", 1),
        placement("acme", "primary", "shared-b", 1),
    ];

    let failure = compose(vec![], &placements, empty_catalog(), &no_history()).expect_err("a duplicate");

    assert_eq!(
        failure,
        ComposeError::DuplicatePlacement {
            tenant: TenantId::try_new("acme").expect("a valid tenant id"),
            logical: LogicalDataSourceName::try_new("primary").expect("a valid logical name"),
        }
    );
}

#[test]
fn held_revisions_are_carried_and_default_to_one_when_absent() {
    let held = PublishedRevisions {
        tenants: Some(DocumentRevision::new(7)),
        data_sources: None,
        catalog: Some(DocumentRevision::new(3)),
    };

    let snapshot = compose(vec![], &[], empty_catalog(), &held).expect("nothing to refuse");

    assert_eq!(snapshot.tenants.revision, DocumentRevision::new(7));
    assert_eq!(snapshot.data_sources.revision, DocumentRevision::new(1));
    assert_eq!(snapshot.catalog.revision, DocumentRevision::new(3));
}

#[test]
fn emptying_is_never_intended_however_empty_the_inputs_are() {
    let snapshot = compose(vec![], &[], empty_catalog(), &no_history()).expect("nothing to refuse");

    assert_eq!(
        snapshot.tenants.emptying,
        fabric_runtime_publication::Emptying::NotIntended
    );
    assert_eq!(
        snapshot.data_sources.emptying,
        fabric_runtime_publication::Emptying::NotIntended
    );
    assert_eq!(
        snapshot.catalog.emptying,
        fabric_runtime_publication::Emptying::NotIntended
    );
}

/// The mutation target: a declaration's `discriminator` must never reach
/// the published data-sources document -- `DataSourceDeclaration::into_document`
/// is the one place that field is dropped, and skipping it (serialising the
/// declaration some other way) is exactly what this proves does not happen.
#[test]
fn into_document_drops_the_discriminator_from_the_published_data_source() {
    let declarations = vec![declaration(
        "shared-a",
        PlacementClassDocument::Shared,
        Some(Discriminator {
            column: FieldName::try_new("tenant_key").expect("a valid field name"),
        }),
    )];

    let snapshot = compose(declarations, &[], empty_catalog(), &no_history()).expect("no duplicates");

    let bytes = data_sources_canonical_json(&snapshot.data_sources.payload).expect("serialises");
    let json = String::from_utf8(bytes).expect("utf-8");
    assert!(!json.contains("discriminator"), "{json}");
    assert!(!json.contains("tenant_key"), "{json}");
}
