//! Item 38: every complexity and size bound is enforced before a connector is
//! ever called, and each one answers with a 400 at the boundary.
//!
//! Every test here proves both halves of the same fact: right at the
//! configured limit the request succeeds, and one past it the request is
//! refused *and* the recording connector never saw it.

mod support;

use std::sync::Arc;

use fabric_connector::DataConnector;
use fabric_data_api::DataApiConfig;
use http::StatusCode;
use serde_json::json;
use support::{
    app_with_config, data_sources, open_permissions, request, resolver, tenants, RecordingConnector,
};
use tower::ServiceExt as _;

fn app_with(config: &DataApiConfig) -> (axum::Router, Arc<RecordingConnector>) {
    let connector = RecordingConnector::new(vec![]);
    let runtime = resolver(tenants(), data_sources());
    let dispatched = Arc::clone(&connector) as Arc<dyn DataConnector>;

    (
        app_with_config(runtime, dispatched, open_permissions(), config),
        connector,
    )
}

#[tokio::test]
async fn a_filter_count_at_the_limit_is_accepted() {
    let (app, connector) = app_with(&DataApiConfig {
        max_filters: 2,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(request("GET", "/customers?a=1&b=2", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(connector.query_count(), 1);
}

#[tokio::test]
async fn a_filter_count_one_over_the_limit_is_rejected() {
    let (app, connector) = app_with(&DataApiConfig {
        max_filters: 2,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(request(
            "GET",
            "/customers?a=1&b=2&c=3",
            json!({"tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_sort_field_count_at_the_limit_is_accepted() {
    let (app, connector) = app_with(&DataApiConfig {
        max_sort_fields: 1,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(request("GET", "/customers?sort=id", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(connector.query_count(), 1);
}

#[tokio::test]
async fn a_sort_field_count_one_over_the_limit_is_rejected() {
    let (app, connector) = app_with(&DataApiConfig {
        max_sort_fields: 1,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(request(
            "GET",
            "/customers?sort=id,name",
            json!({"tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_select_field_count_at_the_limit_is_accepted() {
    let (app, connector) = app_with(&DataApiConfig {
        max_select_fields: 1,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(request(
            "GET",
            "/customers?select=id",
            json!({"tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(connector.query_count(), 1);
}

#[tokio::test]
async fn a_select_field_count_one_over_the_limit_is_rejected() {
    let (app, connector) = app_with(&DataApiConfig {
        max_select_fields: 1,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(request(
            "GET",
            "/customers?select=id,name",
            json!({"tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_filter_depth_at_the_limit_is_accepted() {
    // One filter is depth one: a bare `Compare`, no `And` wrapping it.
    let (app, connector) = app_with(&DataApiConfig {
        max_filter_depth: 1,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(request("GET", "/customers?a=1", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(connector.query_count(), 1);
}

#[tokio::test]
async fn a_filter_depth_one_over_the_limit_is_rejected() {
    // Two filters conjoin into `And { clauses: [.., ..] }`, which is depth
    // two — one past a depth-one ceiling — even though the query language
    // never lets a caller nest anything explicitly.
    let (app, connector) = app_with(&DataApiConfig {
        max_filter_depth: 1,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(request("GET", "/customers?a=1&b=2", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_request_body_at_the_size_limit_is_accepted() {
    let body = json!({"name": "Alice"});
    let limit = u32::try_from(body.to_string().len()).unwrap();

    let (app, connector) = app_with(&DataApiConfig {
        max_request_body_bytes: limit,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(support::json_request(
            "POST",
            "/customers",
            json!({"tenant_id": "acme"}),
            &body,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(connector.mutation_count(), 1);
}

#[tokio::test]
async fn a_request_body_one_byte_over_the_size_limit_is_rejected() {
    let body = json!({"name": "Alice"});
    let limit = u32::try_from(body.to_string().len()).unwrap() - 1;

    let (app, connector) = app_with(&DataApiConfig {
        max_request_body_bytes: limit,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(support::json_request(
            "POST",
            "/customers",
            json!({"tenant_id": "acme"}),
            &body,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn a_mutation_batch_at_the_limit_is_accepted() {
    let (app, connector) = app_with(&DataApiConfig {
        max_mutation_batch_size: 2,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(support::json_request(
            "POST",
            "/customers",
            json!({"tenant_id": "acme"}),
            &json!([{"name": "Alice"}, {"name": "Bob"}]),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(connector.mutation_count(), 1);
}

#[tokio::test]
async fn a_mutation_batch_one_over_the_limit_is_rejected() {
    let (app, connector) = app_with(&DataApiConfig {
        max_mutation_batch_size: 2,
        ..DataApiConfig::default()
    });

    let response = app
        .oneshot(support::json_request(
            "POST",
            "/customers",
            json!({"tenant_id": "acme"}),
            &json!([{"name": "Alice"}, {"name": "Bob"}, {"name": "Carol"}]),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn an_invalid_configuration_is_rejected_before_the_server_starts() {
    let config = DataApiConfig {
        max_filters: 0,
        ..DataApiConfig::default()
    };

    assert!(config.validate().is_err());
}
