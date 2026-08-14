//! Shared fixtures for the Data API integration tests.
//!
//! One place for the recording connector, the tenant and DataSource fixtures,
//! and the router builder. Each test file uses a subset, so unused-item
//! warnings are allowed here rather than in each consumer.

// Each test binary compiles the whole support module but uses a subset of it,
// so unused re-exports here are expected rather than a smell.
#![allow(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

mod connector;
mod delayed_connector;
mod fixtures;
mod requests;
mod scripted_connector;
mod tracing_capture;

pub use connector::RecordingConnector;
pub use delayed_connector::DelayedConnector;
pub use fixtures::{
    acme_data_source, catalog, data_sources, discriminator_data_source, draining_data_source, field,
    read_only_data_source, tenant, tenant_on_draining, tenant_on_replica, tenant_with_missing_data_source,
    tenants,
};
pub use requests::{body_json, json_request, request};
pub use scripted_connector::{wide_row, ScriptedConnector};
pub use tracing_capture::{capture, field_value, CapturedEvent};

use std::sync::Arc;

use axum::Router;
use fabric_connector::{ConnectorRegistry, DataConnector, Row};
use fabric_data_api::{build_data_api, DataApiConfig, ResourcePermissions};
use fabric_identity::{build_identity, IdentityConfig, TrustedIngressReader};
use fabric_tenant_runtime::{
    DataSource, DataSourceRegistry, RuntimeResolver, TenantRegistry, TenantRuntimeBinding,
};
use serde_json::Value;

/// A clock frozen so unsigned test tokens never expire.
pub struct FixedClock;

impl fabric_core::Clock for FixedClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_000
    }
}

/// A row the recording connector will return.
pub fn row(id: i64, name: &str) -> Row {
    Row::new()
        .with(field("id"), Value::from(id))
        .with(field("name"), Value::String(name.to_owned()))
}

/// Builds a resolver over the given state, both registries primed.
pub fn resolver(bindings: Vec<TenantRuntimeBinding>, sources: Vec<DataSource>) -> Arc<RuntimeResolver> {
    let tenant_registry = Arc::new(TenantRegistry::new());
    assert!(
        tenant_registry.apply_all(bindings).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    let source_registry = Arc::new(DataSourceRegistry::new());
    assert!(
        source_registry.apply_all(sources).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    Arc::new(RuntimeResolver::new(tenant_registry, source_registry))
}

/// Builds the assembled router over a given resolver, connector, and Data API
/// configuration — the general form every other `app*` builder in this module
/// delegates to, so a test that needs a non-default limit does not have to
/// duplicate the wiring.
pub fn app_with_config(
    runtime: Arc<RuntimeResolver>,
    connector: Arc<dyn DataConnector>,
    permissions: ResourcePermissions,
    config: &DataApiConfig,
) -> Router {
    // Before any event this crate emits can fire, so no callsite is ever
    // registered against an absent subscriber — see `tracing_capture`'s module
    // docs for the cross-test race this closes.
    tracing_capture::install();

    let identity = build_identity(
        IdentityConfig::default(),
        Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
    )
    .unwrap();

    build_data_api(
        config,
        catalog(),
        permissions,
        runtime,
        ConnectorRegistry::new().with(connector),
        identity,
    )
    .unwrap()
}

/// Builds the assembled router over a given resolver and connector, with the
/// default Data API configuration.
pub fn app_with(
    runtime: Arc<RuntimeResolver>,
    connector: Arc<RecordingConnector>,
    permissions: ResourcePermissions,
) -> Router {
    app_with_config(runtime, connector, permissions, &DataApiConfig::default())
}

/// Permissions with scope checks off — most tests are about tenancy.
pub fn open_permissions() -> ResourcePermissions {
    ResourcePermissions {
        require_scopes: false,
        ..ResourcePermissions::default()
    }
}

/// The standard fixture: two tenants on different placements, two rows.
pub fn app() -> (Router, Arc<RecordingConnector>) {
    let connector = RecordingConnector::new(vec![row(1, "Alice"), row(2, "Bob")]);
    let runtime = resolver(tenants(), data_sources());

    (
        app_with(runtime, Arc::clone(&connector), open_permissions()),
        connector,
    )
}

/// The standard fixture with an empty result set.
pub fn empty_app() -> (Router, Arc<RecordingConnector>) {
    let connector = RecordingConnector::new(vec![]);
    let runtime = resolver(tenants(), data_sources());

    (
        app_with(runtime, Arc::clone(&connector), open_permissions()),
        connector,
    )
}
