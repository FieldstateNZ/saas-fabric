//! The router the process listens on, assembled over the shipped examples.
//!
//! Built by [`fabric_api::startup::compose`], the same join the composition
//! root uses, so a test that asserts what the process serves is asserting it
//! about the real router and not about a copy of it. `composed_surface.rs`
//! records why that matters.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use fabric_api::health::{health_routes, HealthState};
use fabric_api::startup::compose;
use fabric_connector::{ConnectorRegistry, DataConnector};
use fabric_core::SystemClock;
use fabric_data_api::{build_data_api, DataAuthorization};
use fabric_identity::{build_identity, TrustedIngressReader};
use fabric_tenant_runtime::{DataSourceRegistry, RuntimeResolver, TenantRegistry};

use super::{catalog, config, data_sources, tenants};

/// The served router over the example configuration, tenants and DataSources,
/// with the Data API's timeout budget and its one registered connector chosen
/// by the caller.
///
/// The identity reader is the canonical trusted-ingress one, which is what
/// the shipped configuration runs.
pub fn served_router(request_timeout: Duration, connector: Arc<dyn DataConnector>) -> Router {
    let config = config();

    let tenant_registry = Arc::new(TenantRegistry::new());
    assert!(
        tenant_registry.apply_all(tenants()).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    let data_source_registry = Arc::new(DataSourceRegistry::new());
    assert!(
        data_source_registry.apply_all(data_sources()).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    let identity = build_identity(
        config.identity.clone(),
        Arc::new(TrustedIngressReader::new(SystemClock::shared())),
    )
    .expect("the example identity configuration must build");

    let runtime = Arc::new(RuntimeResolver::new(tenant_registry, data_source_registry));

    let connectors = ConnectorRegistry::new().with(connector);

    let data = build_data_api(
        &config.data_api,
        catalog(),
        DataAuthorization::default(),
        Arc::clone(&runtime),
        connectors.clone(),
        Arc::clone(&identity),
    )
    .expect("the Data API must build from the example configuration");

    let health = health_routes(HealthState {
        runtime,
        publication: Arc::default(),
        connectors,
        identity,
        administrator_role: config.health_detail_role.clone(),
    });

    compose(data, health, request_timeout)
}
