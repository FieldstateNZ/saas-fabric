//! The tenant → DataSource chain, driven through the assembled router.
//!
//! What these prove: an application names a logical resource and nothing else,
//! and the platform routes it to that tenant's DataSource.

mod support;

use fabric_connector::{ConnectionName, ConnectionSelector};
use fabric_core::BindingRevision;
use http::StatusCode;
use serde_json::json;
use support::{app, body_json, request, tenant};
use tower::ServiceExt as _;

#[tokio::test]
async fn lists_records_for_the_tenant_in_the_token() {
    let (app, connector) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let (target, _) = connector.last_query();
    assert_eq!(target.tenant(), &tenant("acme"));

    let body = body_json(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"][0]["name"], "Alice");
}

#[tokio::test]
async fn the_application_never_names_a_data_source_but_one_is_selected_for_it() {
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let (target, _) = connector.last_query();

    assert_eq!(target.data_source().as_str(), "acme-prod");
    assert_eq!(
        target.connection(),
        &ConnectionSelector::Named {
            name: ConnectionName::try_new("acme-prod").unwrap()
        }
    );
}

#[tokio::test]
async fn the_same_url_reaches_a_different_data_source_for_a_different_tenant() {
    // §16: one application contract, different physical placement per tenant.
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    let (target, _) = connector.last_query();

    assert_eq!(target.data_source().as_str(), "shared-02");
}

#[tokio::test]
async fn the_target_carries_the_tenant_binding_revision() {
    // Telemetry answers "which tenant binding served this?" — the DataSource
    // has its own, independent revision.
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(connector.last_query().0.revision(), BindingRevision::new(7));
}

#[tokio::test]
async fn the_physical_resource_identifier_names_the_data_source() {
    // §29's telemetry field, and the only place a DataSource id surfaces.
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let identifier = connector.last_query().0.physical_resource_identifier();

    assert!(identifier.starts_with("acme-prod/postgres/"));
}

#[tokio::test]
async fn no_data_source_detail_appears_in_a_successful_response() {
    // §2: applications address logical resources. Nothing physical comes back.
    let (app, _) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let body = body_json(response).await.to_string();

    assert!(!body.contains("acme-prod"));
    assert!(!body.contains("postgres"));
    assert!(!body.contains("data_source"));
}
