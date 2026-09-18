//! One test per rule `select` states, in the order the rustdoc numbers them.

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, FieldName, IsolationModelDocument, PlacementClassDocument,
    PoolSettingsDocument,
};

use super::select;
use crate::{DataIntent, DataSourceDeclaration, Discriminator, PlacementRecord, PlacementRefusal};

const NOW: &str = "2026-09-18T02:14:00Z";

fn tenant(id: &str) -> TenantId {
    TenantId::try_new(id).expect("a valid tenant id")
}

fn logical(name: &str) -> LogicalDataSourceName {
    LogicalDataSourceName::try_new(name).expect("a valid logical data source name")
}

fn source_id(id: &str) -> DataSourceId {
    DataSourceId::try_new(id).expect("a valid data source id")
}

fn intent(class: PlacementClassDocument) -> DataIntent {
    DataIntent {
        class,
        provider: None,
        region: None,
    }
}

/// A declared data source with every filterable field open, so a test that
/// wants a refusal has to close exactly the one gate it is testing.
fn declared(id: &str, placement: PlacementClassDocument) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: source_id(id),
        revision: fabric_core::BindingRevision::new(1),
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
        capabilities: DataSourceCapabilitiesDocument {
            writable: true,
            accepts_new_tenants: true,
        },
        discriminator: if placement == PlacementClassDocument::Shared {
            Some(Discriminator {
                column: FieldName::try_new("tenant_key").expect("a valid field name"),
            })
        } else {
            None
        },
        labels: std::collections::BTreeMap::new(),
    }
}

fn placed(
    tenant_id: &str,
    logical_name: &str,
    data_source: &str,
    isolation: IsolationModelDocument,
) -> PlacementRecord {
    PlacementRecord {
        tenant: tenant(tenant_id),
        logical: logical(logical_name),
        data_source: source_id(data_source),
        isolation,
        placed_at: NOW.to_owned(),
    }
}

// Rule 1: AlreadyPlaced.

#[test]
fn rule_1_a_tenant_already_placed_for_this_logical_is_refused() {
    let held = [placed(
        "acme",
        "primary",
        "shared-a",
        IsolationModelDocument::Database {},
    )];
    let declared_sources = [declared("shared-a", PlacementClassDocument::Dedicated)];

    let failure = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &held,
        NOW,
    )
    .unwrap_err();

    assert_eq!(
        failure,
        PlacementRefusal::AlreadyPlaced {
            tenant: tenant("acme"),
            logical: logical("primary"),
        }
    );
}

#[test]
fn rule_1_the_same_tenant_placed_for_a_different_logical_does_not_block_this_one() {
    let held = [placed(
        "acme",
        "audit",
        "dedicated-a",
        IsolationModelDocument::Database {},
    )];
    let declared_sources = [declared("dedicated-b", PlacementClassDocument::Dedicated)];

    let record = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &held,
        NOW,
    )
    .expect("a different logical is not already placed");

    assert_eq!(record.data_source.as_str(), "dedicated-b");
}

// Rule 2: candidate filters.

#[test]
fn rule_2_a_data_source_of_a_different_class_is_not_a_candidate() {
    let declared_sources = [declared("shared-a", PlacementClassDocument::Shared)];

    let failure = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &[],
        NOW,
    )
    .unwrap_err();

    assert!(matches!(failure, PlacementRefusal::NoDataSourceAdmits { .. }));
}

#[test]
fn rule_2_a_data_source_that_does_not_accept_new_tenants_is_not_a_candidate() {
    let mut source = declared("dedicated-a", PlacementClassDocument::Dedicated);
    source.capabilities.accepts_new_tenants = false;

    let failure = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &[source],
        &[],
        NOW,
    )
    .unwrap_err();

    assert!(matches!(failure, PlacementRefusal::NoDataSourceAdmits { .. }));
}

#[test]
fn rule_2_a_data_source_that_is_not_writable_is_not_a_candidate() {
    let mut source = declared("dedicated-a", PlacementClassDocument::Dedicated);
    source.capabilities.writable = false;

    let failure = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &[source],
        &[],
        NOW,
    )
    .unwrap_err();

    assert!(matches!(failure, PlacementRefusal::NoDataSourceAdmits { .. }));
}

#[test]
fn rule_2_a_stated_region_that_matches_admits_the_candidate() {
    let mut source = declared("dedicated-au", PlacementClassDocument::Dedicated);
    source.residency.region = "au-east".to_owned();

    let mut wanted = intent(PlacementClassDocument::Dedicated);
    wanted.region = Some("au-east".to_owned());

    let record = select(&wanted, &tenant("acme"), &logical("primary"), &[source], &[], NOW)
        .expect("the region matches exactly");

    assert_eq!(record.data_source.as_str(), "dedicated-au");
}

#[test]
fn rule_2_a_stated_region_that_does_not_match_refuses_the_candidate() {
    let mut source = declared("dedicated-nz", PlacementClassDocument::Dedicated);
    source.residency.region = "nz".to_owned();

    let mut wanted = intent(PlacementClassDocument::Dedicated);
    wanted.region = Some("au-east".to_owned());

    let failure = select(&wanted, &tenant("acme"), &logical("primary"), &[source], &[], NOW).unwrap_err();

    assert!(matches!(failure, PlacementRefusal::NoDataSourceAdmits { .. }));
}

#[test]
fn rule_2_no_stated_region_admits_a_candidate_in_any_region() {
    let mut source = declared("dedicated-au", PlacementClassDocument::Dedicated);
    source.residency.region = "au-east".to_owned();

    let record = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &[source],
        &[],
        NOW,
    )
    .expect("no stated region admits every region");

    assert_eq!(record.data_source.as_str(), "dedicated-au");
}

#[test]
fn rule_2_a_stated_provider_is_carried_into_the_refusal_message_and_never_matched() {
    // Nothing a `DataSourceDeclaration` carries could ever match a provider
    // -- the wire has no such field -- so a provider only ever affects the
    // message, never which candidates are considered.
    let mut wanted = intent(PlacementClassDocument::Dedicated);
    wanted.provider = Some("postgres".to_owned());

    let failure = select(&wanted, &tenant("acme"), &logical("primary"), &[], &[], NOW).unwrap_err();

    let PlacementRefusal::NoDataSourceAdmits { provider, .. } = &failure else {
        panic!("expected NoDataSourceAdmits, got {failure:?}");
    };
    assert_eq!(provider.as_deref(), Some("postgres"));
    assert!(failure.to_string().contains("postgres"), "{failure}");
    assert!(failure.to_string().contains("not matched"), "{failure}");
}

// Rule 3: shared placement.

#[test]
fn rule_3_a_shared_candidate_isolates_by_the_tenant_id_as_the_discriminator_value() {
    let declared_sources = [declared("shared-a", PlacementClassDocument::Shared)];

    let record = select(
        &intent(PlacementClassDocument::Shared),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &[],
        NOW,
    )
    .expect("the only candidate admits it");

    assert_eq!(record.data_source.as_str(), "shared-a");
    assert_eq!(
        record.isolation,
        IsolationModelDocument::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "acme".to_owned(),
        }
    );
}

#[test]
fn rule_3_the_same_tenants_second_shared_logical_on_the_same_source_places() {
    // B2: the value is always this tenant's own id, so a tenant with
    // `primary` and `audit` both shared on one source repeats its own
    // value on purpose -- one tenant, one key, in one database. Nothing
    // collides, and this must place rather than being refused as taken.
    let declared_sources = [declared("shared-a", PlacementClassDocument::Shared)];
    let held = [placed(
        "acme",
        "primary",
        "shared-a",
        IsolationModelDocument::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "acme".to_owned(),
        },
    )];

    let record = select(
        &intent(PlacementClassDocument::Shared),
        &tenant("acme"),
        &logical("audit"),
        &declared_sources,
        &held,
        NOW,
    )
    .expect("the same tenant's own value never collides with itself");

    assert_eq!(record.data_source.as_str(), "shared-a");
    assert_eq!(
        record.isolation,
        IsolationModelDocument::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "acme".to_owned(),
        }
    );
}

#[test]
fn rule_3_the_record_carries_whatever_column_the_source_declared_not_a_fixed_one() {
    // N1: nothing about the selector should assume `tenant_key`; the
    // column is read from the candidate's own declaration.
    let mut source = declared("shared-a", PlacementClassDocument::Shared);
    source.discriminator = Some(Discriminator {
        column: FieldName::try_new("customer_id").expect("a valid field name"),
    });

    let record = select(
        &intent(PlacementClassDocument::Shared),
        &tenant("acme"),
        &logical("primary"),
        &[source],
        &[],
        NOW,
    )
    .expect("the only candidate admits it");

    assert_eq!(
        record.isolation,
        IsolationModelDocument::Discriminator {
            column: FieldName::try_new("customer_id").unwrap(),
            value: "acme".to_owned(),
        }
    );
}

#[test]
fn rule_3_the_fewest_held_placements_wins() {
    let declared_sources = [
        declared("shared-busy", PlacementClassDocument::Shared),
        declared("shared-quiet", PlacementClassDocument::Shared),
    ];
    let held = [
        placed(
            "existing-1",
            "primary",
            "shared-busy",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "existing-1".to_owned(),
            },
        ),
        placed(
            "existing-2",
            "primary",
            "shared-busy",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "existing-2".to_owned(),
            },
        ),
    ];

    let record = select(
        &intent(PlacementClassDocument::Shared),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &held,
        NOW,
    )
    .expect("shared-quiet has fewer held placements");

    assert_eq!(record.data_source.as_str(), "shared-quiet");
}

#[test]
fn rule_3_a_tie_on_held_placements_is_broken_by_the_lowest_id() {
    let declared_sources = [
        declared("shared-b", PlacementClassDocument::Shared),
        declared("shared-a", PlacementClassDocument::Shared),
    ];

    let record = select(
        &intent(PlacementClassDocument::Shared),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &[],
        NOW,
    )
    .expect("both are equally empty");

    assert_eq!(record.data_source.as_str(), "shared-a");
}

#[test]
fn rule_3_a_duplicate_discriminator_value_is_refused_though_it_cannot_happen_for_real_tenant_ids() {
    // Impossible in practice -- `value` is always this tenant's own id, and
    // tenant ids are unique by construction, so two *different* tenants can
    // never really collide -- checked anyway, the same discipline
    // `data_sources::held::check_held` applies elsewhere. The held record
    // below belongs to a different tenant ("other") but carries "acme"'s
    // value, standing in for the collision that cannot really happen; B2
    // is what this test is not: the *same* tenant recording its own value
    // twice, once per logical data source, is fine and covered by
    // `rule_1_the_same_tenant_placed_for_a_different_logical_does_not_block_this_one`.
    let declared_sources = [declared("shared-a", PlacementClassDocument::Shared)];
    let held = [placed(
        "other",
        "audit",
        "shared-a",
        IsolationModelDocument::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "acme".to_owned(),
        },
    )];

    let failure = select(
        &intent(PlacementClassDocument::Shared),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &held,
        NOW,
    )
    .unwrap_err();

    assert_eq!(
        failure,
        PlacementRefusal::DiscriminatorValueTaken {
            data_source: source_id("shared-a"),
            value: "acme".to_owned(),
        }
    );
}

// Rule 4: the one-tenant rule for non-shared classes.

#[test]
fn rule_4_a_dedicated_source_with_any_held_placement_is_not_a_candidate() {
    let declared_sources = [declared("dedicated-a", PlacementClassDocument::Dedicated)];
    let held = [placed(
        "existing",
        "primary",
        "dedicated-a",
        IsolationModelDocument::Database {},
    )];

    let failure = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &held,
        NOW,
    )
    .unwrap_err();

    // N2: at least one data source matches the class -- it is just already
    // somebody's -- so this is `AllMatchingSourcesOccupied`, not
    // `NoDataSourceAdmits`, which would send an operator looking for
    // something to declare that they already declared.
    assert_eq!(
        failure,
        PlacementRefusal::AllMatchingSourcesOccupied {
            class: PlacementClassDocument::Dedicated
        }
    );
}

#[test]
fn rule_4_a_free_dedicated_source_is_isolated_as_a_whole_database() {
    let declared_sources = [declared("dedicated-a", PlacementClassDocument::Dedicated)];

    let record = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &[],
        NOW,
    )
    .expect("the source is free");

    assert_eq!(record.isolation, IsolationModelDocument::Database {});
}

#[test]
fn rule_4_a_tie_among_free_candidates_is_broken_by_the_lowest_id() {
    let declared_sources = [
        declared("dedicated-b", PlacementClassDocument::Dedicated),
        declared("dedicated-a", PlacementClassDocument::Dedicated),
    ];

    let record = select(
        &intent(PlacementClassDocument::Dedicated),
        &tenant("acme"),
        &logical("primary"),
        &declared_sources,
        &[],
        NOW,
    )
    .expect("both are free");

    assert_eq!(record.data_source.as_str(), "dedicated-a");
}

// Rule 5: nothing admits it.

#[test]
fn rule_5_no_declared_data_source_at_all_is_refused_naming_the_class_and_region() {
    let mut wanted = intent(PlacementClassDocument::Shared);
    wanted.region = Some("nz".to_owned());

    let failure = select(&wanted, &tenant("acme"), &logical("primary"), &[], &[], NOW).unwrap_err();

    assert_eq!(
        failure.to_string(),
        "declare a shared data source in region nz that accepts new tenants"
    );
}
