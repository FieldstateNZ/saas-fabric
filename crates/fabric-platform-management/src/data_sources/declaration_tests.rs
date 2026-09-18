//! Every rule this declaration enforces, and the fidelity of its published form.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDocument, FieldName, PlacementClassDocument,
    PoolSettingsDocument,
};

use super::{DataSourceDeclaration, Discriminator};
use crate::{DataSourceRule, PoolField};

fn discriminator() -> Discriminator {
    Discriminator {
        column: FieldName::try_new("tenant_key").expect("a valid field name"),
    }
}

fn declaration(
    placement: PlacementClassDocument,
    discriminator: Option<Discriminator>,
) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new("shared-postgres-nz-01").expect("a valid data source id"),
        revision: BindingRevision::new(3),
        connector: ConnectorId::try_new("postgres-nz").expect("a valid connector id"),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").expect("a valid connection name"),
        },
        placement,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: Some("NZ".to_owned()),
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument {
            writable: true,
            accepts_new_tenants: true,
        },
        discriminator,
        labels: BTreeMap::from([("owner".to_owned(), "platform".to_owned())]),
    }
}

fn shared(discriminator: Option<Discriminator>) -> DataSourceDeclaration {
    declaration(PlacementClassDocument::Shared, discriminator)
}

const NOT_SHARED: [PlacementClassDocument; 5] = [
    PlacementClassDocument::Dedicated,
    PlacementClassDocument::HighAvailability,
    PlacementClassDocument::Regulated,
    PlacementClassDocument::Development,
    PlacementClassDocument::Ephemeral,
];

#[test]
fn a_shared_data_source_without_a_discriminator_is_refused() {
    assert_eq!(
        shared(None).validate().unwrap_err(),
        DataSourceRule::SharedNeedsDiscriminator
    );
}

#[test]
fn a_shared_data_source_with_a_discriminator_is_accepted() {
    assert!(shared(Some(discriminator())).validate().is_ok());
}

#[test]
fn every_non_shared_placement_refuses_a_discriminator() {
    for placement in NOT_SHARED {
        let failure = declaration(placement, Some(discriminator()))
            .validate()
            .unwrap_err();

        assert!(
            matches!(failure, DataSourceRule::DiscriminatorOnlyWhenShared { .. }),
            "{placement:?}"
        );
    }
}

#[test]
fn every_non_shared_placement_is_accepted_without_a_discriminator() {
    for placement in NOT_SHARED {
        assert!(declaration(placement, None).validate().is_ok(), "{placement:?}");
    }
}

#[test]
fn a_zero_max_connections_is_refused_by_field_name() {
    let mut declared = shared(Some(discriminator()));
    declared.pool.max_connections = 0;

    assert_eq!(
        declared.validate().unwrap_err(),
        DataSourceRule::ZeroPool {
            field: PoolField::MaxConnections
        }
    );
}

#[test]
fn a_zero_idle_timeout_is_refused_by_field_name() {
    let mut declared = shared(Some(discriminator()));
    declared.pool.idle_timeout_seconds = 0;

    assert_eq!(
        declared.validate().unwrap_err(),
        DataSourceRule::ZeroPool {
            field: PoolField::IdleTimeoutSeconds
        }
    );
}

#[test]
fn a_zero_acquire_timeout_is_refused_by_field_name() {
    let mut declared = shared(Some(discriminator()));
    declared.pool.acquire_timeout_seconds = 0;

    assert_eq!(
        declared.validate().unwrap_err(),
        DataSourceRule::ZeroPool {
            field: PoolField::AcquireTimeoutSeconds
        }
    );
}

#[test]
fn an_empty_label_key_is_refused() {
    let mut declared = shared(Some(discriminator()));
    declared.labels = BTreeMap::from([(String::new(), "platform".to_owned())]);

    assert_eq!(declared.validate().unwrap_err(), DataSourceRule::EmptyLabel);
}

#[test]
fn an_empty_label_value_is_refused() {
    let mut declared = shared(Some(discriminator()));
    declared.labels = BTreeMap::from([("owner".to_owned(), String::new())]);

    assert_eq!(declared.validate().unwrap_err(), DataSourceRule::EmptyLabel);
}

#[test]
fn into_document_round_trips_through_json_into_the_wire_type() {
    let declared = shared(Some(discriminator()));
    let expected = DataSourceDocument {
        id: declared.id.clone(),
        revision: declared.revision,
        connector: declared.connector.clone(),
        connection: declared.connection.clone(),
        placement: declared.placement,
        residency: declared.residency.clone(),
        pool: declared.pool,
        capabilities: declared.capabilities,
        labels: declared.labels.clone(),
    };

    let json = serde_json::to_string(&declared.into_document()).expect("serialises");
    let round_tripped: DataSourceDocument = serde_json::from_str(&json).expect("deserialises");

    assert_eq!(round_tripped, expected);
}

fn with_secret_reference(reference: &str) -> DataSourceDeclaration {
    let mut declared = shared(Some(discriminator()));
    declared.connection = ConnectionSelectorDocument::Secret {
        reference: reference.to_owned(),
    };
    declared
}

#[test]
fn an_empty_secret_reference_is_refused() {
    assert_eq!(
        with_secret_reference("").validate().unwrap_err(),
        DataSourceRule::MalformedSecretReference
    );
}

#[test]
fn a_secret_reference_over_512_bytes_is_refused() {
    let reference = "a".repeat(513);

    assert_eq!(
        with_secret_reference(&reference).validate().unwrap_err(),
        DataSourceRule::MalformedSecretReference
    );
}

#[test]
fn a_secret_reference_exactly_512_bytes_is_accepted() {
    let reference = "a".repeat(512);

    assert!(with_secret_reference(&reference).validate().is_ok());
}

#[test]
fn a_secret_reference_with_whitespace_is_refused() {
    assert_eq!(
        with_secret_reference("tenant/acme data").validate().unwrap_err(),
        DataSourceRule::MalformedSecretReference
    );
}

#[test]
fn a_secret_reference_with_a_control_character_is_refused() {
    assert_eq!(
        with_secret_reference("tenant/acme\u{0007}")
            .validate()
            .unwrap_err(),
        DataSourceRule::MalformedSecretReference
    );
}

#[test]
fn a_well_formed_secret_reference_is_accepted() {
    assert!(with_secret_reference("tenant/acme/data-primary")
        .validate()
        .is_ok());
}

#[test]
fn a_default_connection_is_refused() {
    let mut declared = shared(Some(discriminator()));
    declared.connection = ConnectionSelectorDocument::Default {};

    assert_eq!(
        declared.validate().unwrap_err(),
        DataSourceRule::ConnectionKindNotDeclarable
    );
}
