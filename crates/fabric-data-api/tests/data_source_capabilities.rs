//! What a DataSource permits, enforced before the connector is reached.
//!
//! Distinct from connector capabilities: the connector supports mutations
//! perfectly well here, and the platform still says no. Either saying no is a
//! no, and both fail closed (§28).

mod support;

use std::sync::Arc;

use http::StatusCode;
use serde_json::json;
use support::{
    app_with, body_json, data_sources, json_request, open_permissions, read_only_data_source, request,
    resolver, tenant_on_replica, tenants, RecordingConnector,
};
use tower::ServiceExt as _;

/// A deployment where one tenant sits on a read-only replica.
fn app_with_replica() -> (axum::Router, Arc<RecordingConnector>) {
    let mut bindings = tenants();
    bindings.push(tenant_on_replica());

    let mut sources = data_sources();
    sources.push(read_only_data_source());

    let connector = RecordingConnector::new(vec![]);

    (
        app_with(
            resolver(bindings, sources),
            Arc::clone(&connector),
            open_permissions(),
        ),
        connector,
    )
}

#[tokio::test]
async fn a_write_to_a_read_only_data_source_is_refused_before_it_reaches_the_connector() {
    // The DataSource capability check. The connector supports mutations
    // perfectly well; the platform still says no.
    let (app, connector) = app_with_replica();

    let response = app
        .oneshot(json_request(
            "POST",
            "/customers",
            json!({"tenant_id": "reader"}),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(connector.mutation_count(), 0);

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "read_only");
    // Which DataSource, and why, stays internal.
    assert!(!body.to_string().contains("replica-01"));
}

#[tokio::test]
async fn reads_from_a_read_only_data_source_still_work() {
    let (app, connector) = app_with_replica();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "reader"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(connector.query_count(), 1);
}
