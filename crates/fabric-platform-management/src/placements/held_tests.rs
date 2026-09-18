//! A held placements document a break-glass edit could have made incoherent.

use std::collections::BTreeMap;

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, FieldName, IsolationModelDocument, PlacementClassDocument,
    PoolSettingsDocument,
};

use super::check_held_placements;
use crate::{DataSourceDeclaration, Discriminator, PlacementRecord, PlatformError};

fn source(id: &str, placement: PlacementClassDocument) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).expect("a valid data source id"),
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
        capabilities: DataSourceCapabilitiesDocument::default(),
        discriminator: if placement == PlacementClassDocument::Shared {
            Some(Discriminator {
                column: FieldName::try_new("tenant_key").expect("a valid field name"),
            })
        } else {
            None
        },
        labels: BTreeMap::new(),
    }
}

fn record(
    tenant: &str,
    logical: &str,
    data_source: &str,
    isolation: IsolationModelDocument,
) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).expect("a valid tenant id"),
        logical: LogicalDataSourceName::try_new(logical).expect("a valid logical data source name"),
        data_source: DataSourceId::try_new(data_source).expect("a valid data source id"),
        isolation,
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    }
}

fn detail(failure: PlatformError) -> String {
    let PlatformError::InvalidHeldPlacements { detail } = failure else {
        panic!("expected InvalidHeldPlacements, got {failure:?}");
    };
    detail
}

#[test]
fn a_coherent_document_is_accepted() {
    let declared = [
        source("shared-a", PlacementClassDocument::Shared),
        source("dedicated-a", PlacementClassDocument::Dedicated),
    ];
    let placements = [
        record(
            "acme",
            "primary",
            "shared-a",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "acme".to_owned(),
            },
        ),
        record(
            "acme",
            "audit",
            "dedicated-a",
            IsolationModelDocument::Database {},
        ),
    ];

    assert!(check_held_placements(&placements, &declared).is_ok());
}

#[test]
fn a_duplicate_tenant_and_logical_pair_is_refused() {
    let declared = [
        source("dedicated-a", PlacementClassDocument::Dedicated),
        source("dedicated-b", PlacementClassDocument::Dedicated),
    ];
    let placements = [
        record(
            "acme",
            "primary",
            "dedicated-a",
            IsolationModelDocument::Database {},
        ),
        record(
            "acme",
            "primary",
            "dedicated-b",
            IsolationModelDocument::Database {},
        ),
    ];

    let failure = check_held_placements(&placements, &declared).unwrap_err();
    let detail = detail(failure);
    assert!(detail.contains("acme"), "{detail}");
    assert!(
        !detail.contains('/'),
        "no path in an operator-facing message: {detail}"
    );
}

#[test]
fn a_data_source_nothing_declares_is_refused() {
    let placements = [record(
        "acme",
        "primary",
        "missing",
        IsolationModelDocument::Database {},
    )];

    let failure = check_held_placements(&placements, &[]).unwrap_err();
    let detail = detail(failure);
    assert!(detail.contains("acme"), "{detail}");
}

#[test]
fn a_discriminator_isolation_on_a_non_shared_source_is_refused() {
    let declared = [source("dedicated-a", PlacementClassDocument::Dedicated)];
    let placements = [record(
        "acme",
        "primary",
        "dedicated-a",
        IsolationModelDocument::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "acme".to_owned(),
        },
    )];

    let failure = check_held_placements(&placements, &declared).unwrap_err();
    let detail = detail(failure);
    assert!(detail.contains("acme"), "{detail}");
    assert!(detail.contains("dedicated-a"), "{detail}");
}

#[test]
fn a_database_isolation_on_a_shared_source_is_refused() {
    let declared = [source("shared-a", PlacementClassDocument::Shared)];
    let placements = [record(
        "acme",
        "primary",
        "shared-a",
        IsolationModelDocument::Database {},
    )];

    let failure = check_held_placements(&placements, &declared).unwrap_err();
    assert!(matches!(failure, PlatformError::InvalidHeldPlacements { .. }));
}

#[test]
fn a_discriminator_column_that_does_not_match_the_sources_declared_column_is_refused() {
    // B3: the isolation kind is right (discriminator, on a shared source)
    // but the column is not the one the source declared.
    let declared = [source("shared-a", PlacementClassDocument::Shared)];
    let placements = [record(
        "acme",
        "primary",
        "shared-a",
        IsolationModelDocument::Discriminator {
            column: FieldName::try_new("customer_id").unwrap(),
            value: "acme".to_owned(),
        },
    )];

    let failure = check_held_placements(&placements, &declared).unwrap_err();
    let detail = detail(failure);
    assert!(detail.contains("acme"), "{detail}");
    assert!(detail.contains("customer_id"), "{detail}");
}

#[test]
fn a_second_non_discriminator_placement_on_one_source_is_refused() {
    // B3: a non-shared source is one tenant's, so a second whole-database
    // placement naming it -- even for a different tenant -- is refused.
    let declared = [source("dedicated-a", PlacementClassDocument::Dedicated)];
    let placements = [
        record(
            "acme",
            "primary",
            "dedicated-a",
            IsolationModelDocument::Database {},
        ),
        record(
            "initech",
            "primary",
            "dedicated-a",
            IsolationModelDocument::Database {},
        ),
    ];

    let failure = check_held_placements(&placements, &declared).unwrap_err();
    let detail = detail(failure);
    assert!(detail.contains("dedicated-a"), "{detail}");
}

#[test]
fn a_schema_isolation_matches_no_placement_class() {
    let declared = [source("dedicated-a", PlacementClassDocument::Dedicated)];
    let placements = [record(
        "acme",
        "primary",
        "dedicated-a",
        IsolationModelDocument::Schema {
            schema: fabric_runtime_publication::SchemaName::try_new("acme").unwrap(),
        },
    )];

    let failure = check_held_placements(&placements, &declared).unwrap_err();
    assert!(matches!(failure, PlatformError::InvalidHeldPlacements { .. }));
}

#[test]
fn two_tenants_recorded_with_one_discriminator_value_are_refused() {
    let declared = [source("shared-a", PlacementClassDocument::Shared)];
    let placements = [
        record(
            "acme",
            "primary",
            "shared-a",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "shared-value".to_owned(),
            },
        ),
        record(
            "initech",
            "primary",
            "shared-a",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "shared-value".to_owned(),
            },
        ),
    ];

    let failure = check_held_placements(&placements, &declared).unwrap_err();
    let detail = detail(failure);
    assert!(detail.contains("shared-a"), "{detail}");
    assert!(detail.contains("shared-value"), "{detail}");
}

#[test]
fn the_same_discriminator_value_on_two_different_data_sources_is_fine() {
    let declared = [
        source("shared-a", PlacementClassDocument::Shared),
        source("shared-b", PlacementClassDocument::Shared),
    ];
    let placements = [
        record(
            "acme",
            "primary",
            "shared-a",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "acme".to_owned(),
            },
        ),
        record(
            "acme",
            "audit",
            "shared-b",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "acme".to_owned(),
            },
        ),
    ];

    assert!(check_held_placements(&placements, &declared).is_ok());
}

#[test]
fn one_tenants_two_shared_logicals_on_one_shared_source_with_one_value_is_fine() {
    // B2's held-side twin: `check_held_placements`'s same-tenant exclusion
    // must accept this, not merely fail to refuse it by accident. If the
    // exclusion regressed to "any second entry naming this (source, value)
    // pair is refused", this is exactly the document it would wrongly
    // refuse.
    let declared = [source("shared-a", PlacementClassDocument::Shared)];
    let placements = [
        record(
            "acme",
            "primary",
            "shared-a",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "acme".to_owned(),
            },
        ),
        record(
            "acme",
            "audit",
            "shared-a",
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "acme".to_owned(),
            },
        ),
    ];

    assert!(check_held_placements(&placements, &declared).is_ok());
}

#[test]
fn a_placed_at_that_does_not_parse_as_rfc_3339_is_refused() {
    // A break-glass `placed_at: ""` (or any other non-date text) would
    // otherwise render as an invalid date wherever this record is shown.
    let declared = [source("dedicated-a", PlacementClassDocument::Dedicated)];
    let mut placements = [record(
        "acme",
        "primary",
        "dedicated-a",
        IsolationModelDocument::Database {},
    )];
    placements[0].placed_at = String::new();

    let failure = check_held_placements(&placements, &declared).unwrap_err();
    let detail = detail(failure);
    assert!(detail.contains("acme"), "{detail}");
    assert!(detail.contains("RFC 3339"), "{detail}");
}
