//! Item 40: nothing physical — no collection name, no connector scalar type,
//! no tenant schema name — ever reaches a response body.
//!
//! The catalogue fixture's `readOnlyReport` resource is the load-bearing bit
//! here: its *logical* name is `readOnlyReport`, but it maps to the physical
//! `customers` collection — the same collection the `customers` resource
//! uses. Because the two names differ, a response that leaked the physical
//! `CollectionName` would be caught by a plain substring check, unlike the
//! `customers` fixture where the logical and physical names happen to
//! coincide and a leak would hide in plain sight.

mod support;

use http::StatusCode;
use serde_json::json;
use support::{app, body_json, json_request, request};
use tower::ServiceExt as _;

#[tokio::test]
async fn a_response_for_a_renamed_resource_never_names_its_physical_collection() {
    let (app, _) = app();

    let response = app
        .oneshot(request("GET", "/readOnlyReport", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await.to_string();

    // "customers" is the physical collection `readOnlyReport` is bound to —
    // and is not, itself, a value any row in the fixture carries.
    assert!(!body.contains("customers"));
}

#[tokio::test]
async fn a_list_response_contains_only_logical_shape_keys() {
    // Every key in a list response is a platform-defined contract term
    // (`data`, `paging`, `limit`, `offset`, `returned`, `has_more`) or a
    // field the row itself carries. None of it is a schema name.
    let (app, _) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();
    let body = body_json(response).await;

    let top_level: Vec<&String> = body.as_object().unwrap().keys().collect();
    assert_eq!(top_level, vec!["data", "paging"]);

    let paging: Vec<&String> = body["paging"].as_object().unwrap().keys().collect();
    let mut paging_sorted = paging.clone();
    paging_sorted.sort();
    assert_eq!(paging_sorted, vec!["has_more", "limit", "offset", "returned"]);
}

#[tokio::test]
async fn a_write_response_never_names_the_data_source_or_connector() {
    let (app, _) = app();

    let response = app
        .oneshot(json_request(
            "POST",
            "/customers",
            json!({"tenant_id": "acme"}),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = body_json(response).await.to_string();

    assert!(!body.contains("acme-prod"));
    assert!(!body.contains("postgres"));
    assert!(!body.to_lowercase().contains("data_source"));
}

#[tokio::test]
async fn an_internal_error_body_carries_no_physical_detail() {
    // Reuses the missing-DataSource scenario: the response must stay generic
    // even though the platform-side error names a DataSource id internally.
    let mut bindings = support::tenants();
    bindings.push(support::tenant_with_missing_data_source());

    let connector = support::RecordingConnector::new(vec![]);
    let app = support::app_with(
        support::resolver(bindings, support::data_sources()),
        std::sync::Arc::clone(&connector),
        support::open_permissions(),
    );

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "orphan"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(connector.query_count(), 0);

    let body = body_json(response).await;

    assert_eq!(body["error"]["message"], "internal error");
    let full = body.to_string();
    assert!(!full.contains("never-deployed"));
    assert!(!full.contains("postgres"));

    // The only two keys a caller sees on an error, besides the message and
    // stable code, are the ones this contract defines.
    let error_keys: Vec<&String> = body["error"].as_object().unwrap().keys().collect();
    let mut sorted = error_keys.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["code", "message", "request_id"]);
}
