//! Every way this fails, and what the caller is told.

mod support;

use std::sync::Arc;

use fabric_data_api::ResourcePermissions;
use fabric_tenant_runtime::{DataSourceRegistry, RuntimeResolver, TenantRegistry};
use http::StatusCode;
use serde_json::json;
use support::{
    app, app_with, body_json, data_sources, json_request, open_permissions, request, resolver, tenants,
    RecordingConnector,
};
use tower::ServiceExt as _;

#[tokio::test]
async fn an_unknown_tenant_is_refused_without_reaching_a_connector() {
    let (app, connector) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "ghost"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(connector.query_count(), 0);

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "unknown_tenant");
    // The tenant is not echoed back.
    assert!(!body["error"]["message"].as_str().unwrap().contains("ghost"));
}

#[tokio::test]
async fn an_unprimed_runtime_returns_service_unavailable_not_forbidden() {
    // §28: a cold start must not tell every caller their tenant is gone.
    let connector = RecordingConnector::new(vec![]);
    let runtime = Arc::new(RuntimeResolver::new(
        Arc::new(TenantRegistry::new()),
        Arc::new(DataSourceRegistry::new()),
    ));

    let app = app_with(runtime, Arc::clone(&connector), open_permissions());

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(response).await["error"]["code"], "runtime_unavailable");
}

#[tokio::test]
async fn an_unprimed_data_source_registry_alone_is_enough_to_be_unavailable() {
    // Tenants loaded, DataSources not: the chain cannot complete.
    let tenant_registry = Arc::new(TenantRegistry::new());
    assert!(
        tenant_registry.apply_all(tenants()).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    let runtime = Arc::new(RuntimeResolver::new(
        tenant_registry,
        Arc::new(DataSourceRegistry::new()),
    ));

    let connector = RecordingConnector::new(vec![]);
    let app = app_with(runtime, Arc::clone(&connector), open_permissions());

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_binding_pointing_at_a_missing_data_source_is_an_internal_error() {
    // A reconciliation gap, not a caller error — and the DataSource id must not
    // travel back to the caller.
    let mut bindings = tenants();
    bindings.push(support::tenant_with_missing_data_source());

    let connector = RecordingConnector::new(vec![]);
    let app = app_with(
        resolver(bindings, data_sources()),
        Arc::clone(&connector),
        open_permissions(),
    );

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "orphan"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(connector.query_count(), 0);

    let body = body_json(response).await.to_string();
    assert!(!body.contains("never-deployed"));
}

#[tokio::test]
async fn an_uncatalogued_resource_is_a_404() {
    let (app, _) = app();

    let response = app
        .oneshot(request("GET", "/invoices", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_operation_the_catalogue_does_not_expose_is_refused() {
    let (app, connector) = app();

    let response = app
        .oneshot(json_request(
            "POST",
            "/readOnlyReport",
            json!({"tenant_id": "acme"}),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "operation_not_allowed"
    );
    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn a_scope_check_refuses_an_unauthorised_operation() {
    let connector = RecordingConnector::new(vec![]);
    let app = app_with(
        resolver(tenants(), data_sources()),
        Arc::clone(&connector),
        ResourcePermissions::default(),
    );

    let response = app
        .oneshot(request(
            "DELETE",
            "/customers/1",
            json!({"tenant_id": "acme", "scope": "data:customers:read"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(connector.mutation_count(), 0);
}
