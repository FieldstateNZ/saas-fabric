//! Migration and independent reconciliation, seen from the request path.
//!
//! These are the tests that justify making DataSources first-class: a physical
//! change reaches every tenant on that DataSource without rewriting a single
//! tenant binding.

mod support;

use std::sync::Arc;

use fabric_connector::{ConnectionName, ConnectionSelector, ConnectorId, IsolationModel};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName};
use fabric_tenant_runtime::{
    DataSourceRegistry, RuntimeResolver, TenantDataBinding, TenantRegistry, TenantRuntimeBinding,
};
use serde_json::json;
use support::{
    acme_data_source, app_with, data_sources, discriminator_data_source, open_permissions, request, tenant,
    tenants, RecordingConnector,
};
use tower::ServiceExt as _;

/// Builds an app over registries the test keeps handles to.
fn app_over(
    tenant_registry: &Arc<TenantRegistry>,
    source_registry: &Arc<DataSourceRegistry>,
) -> (axum::Router, Arc<RecordingConnector>) {
    let connector = RecordingConnector::new(vec![]);
    let runtime = Arc::new(RuntimeResolver::new(
        Arc::clone(tenant_registry),
        Arc::clone(source_registry),
    ));

    (
        app_with(runtime, Arc::clone(&connector), open_permissions()),
        connector,
    )
}

fn registries() -> (Arc<TenantRegistry>, Arc<DataSourceRegistry>) {
    let tenant_registry = Arc::new(TenantRegistry::new());
    assert!(
        tenant_registry.apply_all(tenants()).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    let source_registry = Arc::new(DataSourceRegistry::new());
    assert!(
        source_registry.apply_all(data_sources()).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    (tenant_registry, source_registry)
}

#[tokio::test]
async fn changing_a_data_source_moves_every_tenant_on_it_with_no_tenant_edit() {
    // The payoff of the model: one edit to one resource.
    let (tenant_registry, source_registry) = registries();
    let (app, connector) = app_over(&tenant_registry, &source_registry);

    let mut moved = acme_data_source();
    moved.revision = BindingRevision::new(5);
    moved.connection = ConnectionSelector::Named {
        name: ConnectionName::try_new("acme-db-01").unwrap(),
    };
    assert!(source_registry.apply_one(moved));

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let (target, _) = connector.last_query();
    assert_eq!(
        target.connection(),
        &ConnectionSelector::Named {
            name: ConnectionName::try_new("acme-db-01").unwrap()
        }
    );
    // The tenant binding was never rewritten.
    assert_eq!(target.tenant_revision(), BindingRevision::new(7));
}

#[tokio::test]
async fn rebinding_a_tenant_moves_it_to_a_different_data_source() {
    // §19's migration: provision the target, then publish revision N+1 of the
    // tenant binding. The application keeps calling /customers.
    let (tenant_registry, source_registry) = registries();
    let (app, connector) = app_over(&tenant_registry, &source_registry);

    let rebound = TenantRuntimeBinding::new(tenant("acme"), BindingRevision::new(8)).with_data(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBinding::new(
            DataSourceId::try_new("shared-02").unwrap(),
            IsolationModel::Discriminator {
                column: support::field("tenant_key"),
                value: "tenant-901".to_owned(),
            },
        ),
    );
    assert!(tenant_registry.apply_one(rebound));

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let (target, spec) = connector.last_query();

    assert_eq!(target.data_source().as_str(), "shared-02");
    assert_eq!(target.tenant_revision(), BindingRevision::new(8));
    // Moving onto a shared table brings the isolation predicate with it.
    assert!(spec.filter.is_some());
}

#[tokio::test]
async fn a_stale_tenant_update_is_ignored_by_the_request_path() {
    let (tenant_registry, source_registry) = registries();
    let (app, connector) = app_over(&tenant_registry, &source_registry);

    let stale = TenantRuntimeBinding::new(tenant("acme"), BindingRevision::new(2)).with_data(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBinding::new(
            DataSourceId::try_new("shared-02").unwrap(),
            IsolationModel::Database,
        ),
    );
    assert!(!tenant_registry.apply_one(stale));

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let (target, _) = connector.last_query();
    assert_eq!(target.tenant_revision(), BindingRevision::new(7));
    assert_eq!(target.data_source().as_str(), "acme-prod");
}

#[tokio::test]
async fn a_stale_data_source_update_is_ignored_too() {
    let (tenant_registry, source_registry) = registries();
    let (app, connector) = app_over(&tenant_registry, &source_registry);

    let mut stale = acme_data_source();
    stale.revision = BindingRevision::new(1);
    stale.connector = ConnectorId::try_new("retired").unwrap();
    assert!(!source_registry.apply_one(stale));

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(connector.last_query().0.connector().as_str(), "postgres");
}

#[tokio::test]
async fn removing_a_data_source_makes_its_tenants_fail_closed() {
    // Deprovisioning a DataSource with tenants still bound is a reconciliation
    // error. It must not silently route them somewhere else.
    let (tenant_registry, source_registry) = registries();
    let (app, connector) = app_over(&tenant_registry, &source_registry);

    assert!(source_registry.invalidate(&DataSourceId::try_new("acme-prod").unwrap()));

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn one_tenant_being_broken_does_not_affect_another() {
    let (tenant_registry, source_registry) = registries();
    let (app, connector) = app_over(&tenant_registry, &source_registry);

    assert!(source_registry.invalidate(&DataSourceId::try_new("acme-prod").unwrap()));

    // globex is on a different DataSource and keeps working.
    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(connector.last_query().0.data_source().as_str(), "shared-02");
    assert_eq!(discriminator_data_source().id.as_str(), "shared-02");
}
