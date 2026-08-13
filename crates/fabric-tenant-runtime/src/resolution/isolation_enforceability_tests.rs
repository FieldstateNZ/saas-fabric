//! Structural isolation must come from somewhere structural.
//!
//! `Database` and `Schema` isolation contribute no predicate — the separation
//! is meant to come from the connection reaching a different database or a
//! different schema. A [`DataSource`] carries exactly one
//! `ConnectionSelector`, shared by every tenant bound to it. Put those two
//! facts together on a DataSource that reconciliation may place many tenants
//! on, and every one of them issues the same unfiltered query against the same
//! connection.
//!
//! That is not weakened isolation. It is none, and it is silent: two tenants
//! read each other's rows and delete each other's records with no error
//! anywhere. These tests are the reason it cannot happen.
//!
//! The rule they pin: a `Shared` DataSource may only serve `Discriminator`
//! isolation, because that is the only model carrying its own predicate.

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_connector::{
    ConnectionName, ConnectionSelector, ConnectorId, FieldName, IsolationModel, SchemaName,
};
use fabric_core::BindingRevision;

use crate::data_source::{DataResidency, DataSourceCapabilities, PlacementClass, PoolSettings};
use crate::tenant::TenantDataBinding;
use crate::testing::{data_source_id, primary, tenant};
use crate::{
    DataSource, DataSourceRegistry, ResolveError, RuntimeResolver, TenantRegistry, TenantRuntimeBinding,
};

fn data_source(id: &str, placement: PlacementClass) -> DataSource {
    DataSource {
        id: data_source_id(id),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelector::Named {
            name: ConnectionName::try_new("shared-connection").unwrap(),
        },
        placement,
        residency: DataResidency::in_region("au-east"),
        pool: PoolSettings::default(),
        capabilities: DataSourceCapabilities {
            writable: true,
            accepts_new_tenants: true,
        },
        labels: BTreeMap::new(),
    }
}

fn schema_isolation(name: &str) -> IsolationModel {
    IsolationModel::Schema {
        schema: SchemaName::try_new(name).unwrap(),
    }
}

fn discriminator_isolation() -> IsolationModel {
    IsolationModel::Discriminator {
        column: FieldName::try_new("tenant_key").unwrap(),
        value: "tenant-482".to_owned(),
    }
}

fn resolve(
    placement: PlacementClass,
    isolation: IsolationModel,
) -> Result<crate::ResolvedDataSource, ResolveError> {
    let tenants = Arc::new(TenantRegistry::new());
    tenants
        .apply_all(vec![TenantRuntimeBinding::new(
            tenant("acme"),
            BindingRevision::new(7),
        )
        .with_data(
            primary(),
            TenantDataBinding::new(data_source_id("pg-01"), isolation),
        )])
        .unwrap();

    let sources = Arc::new(DataSourceRegistry::new());
    sources.apply_all(vec![data_source("pg-01", placement)]).unwrap();

    RuntimeResolver::new(tenants, sources).resolve_data_source(&tenant("acme"), &primary())
}

// -- Shared + structural isolation: refused ---------------------------------

#[test]
fn schema_isolation_on_a_shared_data_source_is_refused() {
    // The exact configuration that shipped in `examples/tenants.json` before
    // this check existed, and the one an operator reaches for when they read
    // "shared database with per-tenant schemas" in the specification.
    let error = resolve(PlacementClass::Shared, schema_isolation("globex")).unwrap_err();

    assert!(
        matches!(error, ResolveError::IsolationNotEnforceable { .. }),
        "{error:?}"
    );
}

#[test]
fn database_isolation_on_a_shared_data_source_is_refused() {
    // Same reasoning, and worth its own test: `Database` looks the safest of
    // the three models precisely because it sounds like a whole separate
    // database, which makes it the one most likely to be paired with a shared
    // DataSource by mistake.
    let error = resolve(PlacementClass::Shared, IsolationModel::Database).unwrap_err();

    assert!(matches!(error, ResolveError::IsolationNotEnforceable { .. }));
}

#[test]
fn the_refusal_names_the_tenant_the_data_source_and_the_model() {
    // An operator reading this in a log needs all three to act: which binding
    // is wrong, which DataSource it points at, and what it asked for.
    let error = resolve(PlacementClass::Shared, schema_isolation("globex")).unwrap_err();
    let message = error.to_string();

    assert!(message.contains("acme"), "{message}");
    assert!(message.contains("pg-01"), "{message}");
    assert!(message.contains("schema"), "{message}");
}

#[test]
fn the_refusal_does_not_disclose_the_schema_name_or_the_connection() {
    // The message reaches an error response. A schema name is a physical
    // placement detail, and §26 keeps those behind the Data API.
    let message = resolve(PlacementClass::Shared, schema_isolation("globex"))
        .unwrap_err()
        .to_string();

    assert!(!message.contains("globex"), "{message}");
    assert!(!message.contains("shared-connection"), "{message}");
}

// -- Shared + discriminator: allowed ----------------------------------------

#[test]
fn discriminator_isolation_on_a_shared_data_source_is_allowed() {
    // The whole point of the rule. Discriminator isolation carries its own
    // predicate, so sharing a connection is safe — and refusing it here would
    // break the one model shared placement exists to serve.
    let resolved = resolve(PlacementClass::Shared, discriminator_isolation()).unwrap();

    assert_eq!(resolved.data_source.id.as_str(), "pg-01");
}

// -- Non-shared placement: structural isolation is fine ---------------------

#[test]
fn structural_isolation_is_allowed_on_every_non_shared_placement() {
    for placement in [
        PlacementClass::Dedicated,
        PlacementClass::HighAvailability,
        PlacementClass::Regulated,
        PlacementClass::Development,
        PlacementClass::Ephemeral,
    ] {
        assert!(
            resolve(placement, IsolationModel::Database).is_ok(),
            "database isolation must be allowed on {placement:?}"
        );
        assert!(
            resolve(placement, schema_isolation("acme")).is_ok(),
            "schema isolation must be allowed on {placement:?}"
        );
    }
}

// -- The check refuses; it never re-selects ---------------------------------

#[test]
fn the_check_refuses_rather_than_choosing_a_different_data_source() {
    // Placement is read here, which is the single exception to the rule the
    // sibling `placement_inertness_tests` pin. The exception is narrow: it can
    // veto, never choose. With a perfectly good dedicated DataSource also
    // registered, the answer is still an error and not that other DataSource.
    let tenants = Arc::new(TenantRegistry::new());
    tenants
        .apply_all(vec![TenantRuntimeBinding::new(
            tenant("acme"),
            BindingRevision::new(7),
        )
        .with_data(
            primary(),
            TenantDataBinding::new(data_source_id("pg-01"), IsolationModel::Database),
        )])
        .unwrap();

    let sources = Arc::new(DataSourceRegistry::new());
    sources
        .apply_all(vec![
            data_source("pg-01", PlacementClass::Shared),
            data_source("pg-02", PlacementClass::Dedicated),
        ])
        .unwrap();

    let error = RuntimeResolver::new(tenants, sources)
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap_err();

    assert!(matches!(error, ResolveError::IsolationNotEnforceable { .. }));
}
