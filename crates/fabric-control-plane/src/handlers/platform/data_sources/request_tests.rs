use std::collections::BTreeMap;

use fabric_platform_management::{ConnectionName, PlacementClassDocument};

use super::*;

fn input(placement: &str) -> DataSourceInput {
    DataSourceInput {
        connector: ConnectorId::try_new("postgres-nz").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").unwrap(),
        },
        placement: placement.to_owned(),
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: Some("NZ".to_owned()),
        },
        pool: PoolInput {
            max_connections: 20,
            idle_timeout_seconds: 300,
            acquire_timeout_seconds: 5,
        },
        capabilities: CapabilitiesInput {
            writable: true,
            accepts_new_tenants: true,
        },
        discriminator: Some(Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
        }),
        labels: BTreeMap::new(),
    }
}

#[test]
fn a_well_formed_input_becomes_a_declaration_for_the_path_id() {
    let id = DataSourceId::try_new("shared-postgres-nz-01").unwrap();

    let declaration = input("shared").into_declaration(id.clone()).unwrap();

    assert_eq!(declaration.id, id);
    assert_eq!(declaration.placement, PlacementClassDocument::Shared);
    assert_eq!(declaration.pool.max_connections, 20);
    assert!(declaration.discriminator.is_some());
}

#[test]
fn the_incoming_revision_is_never_trusted() {
    // There is no `revision` field to send in the first place -- this pins
    // that `into_declaration` never reads one from anywhere but its own
    // placeholder, which `DataSources::declare` then discards regardless.
    let declaration = input("shared")
        .into_declaration(DataSourceId::try_new("sql-01").unwrap())
        .unwrap();

    assert_eq!(declaration.revision, BindingRevision::new(0));
}

#[test]
fn an_unrecognised_placement_word_is_refused_before_any_field_is_built() {
    let error = input("not-a-placement")
        .into_declaration(DataSourceId::try_new("sql-01").unwrap())
        .unwrap_err();

    assert!(matches!(error, ControlPlaneError::InvalidRequest(_)));
}

/// A well-formed body, as JSON -- for the two tests below, which need to
/// go through `Deserialize` itself rather than build a `DataSourceInput`
/// directly, since `deny_unknown_fields` is a property of deserialising,
/// not of the type's fields.
fn well_formed_body() -> serde_json::Value {
    serde_json::json!({
        "connector": "postgres-nz",
        "connection": {"kind": "named", "name": "shared"},
        "placement": "shared",
        "residency": {"region": "nz", "jurisdiction": "NZ"},
        "pool": {"maxConnections": 20, "idleTimeoutSeconds": 300, "acquireTimeoutSeconds": 5},
        "capabilities": {"writable": true, "acceptsNewTenants": true},
        "discriminator": {"column": "tenant_key"},
        "labels": {},
    })
}

#[test]
fn an_id_field_is_refused_rather_than_silently_ignored() {
    // The id is the path, never the body -- see this struct's own doc
    // comment. A body that carries one anyway (a browser sending back a
    // read verbatim, say) must be refused, not have the field quietly
    // dropped.
    let mut body = well_formed_body();
    body["id"] = serde_json::json!("shared-postgres-nz-01");

    assert!(serde_json::from_value::<DataSourceInput>(body).is_err());
}

#[test]
fn a_revision_field_is_refused_rather_than_silently_ignored() {
    // The revision is computed by `DataSources::declare`, never trusted
    // from the caller -- see `the_incoming_revision_is_never_trusted`
    // above. A body that sends one anyway is refused the same way an id
    // is, rather than silently discarded.
    let mut body = well_formed_body();
    body["revision"] = serde_json::json!(7);

    assert!(serde_json::from_value::<DataSourceInput>(body).is_err());
}
