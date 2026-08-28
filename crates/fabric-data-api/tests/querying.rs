//! Paging, filtering, projection.

mod support;

use http::StatusCode;
use serde_json::json;
use support::{app, body_json, empty_app, request};
use tower::ServiceExt as _;

#[tokio::test]
async fn paging_asks_for_one_row_beyond_the_page() {
    let (app, connector) = app();

    app.oneshot(request(
        "GET",
        "/customers?limit=1&offset=10",
        json!({"tenant_id": "acme"}),
    ))
    .await
    .unwrap();

    let (_, spec) = connector.last_query();

    // The probe row is what makes `has_more` a fact rather than a guess.
    assert_eq!(spec.limit, Some(2));
    assert_eq!(spec.offset, Some(10));
}

#[tokio::test]
async fn a_full_page_reports_that_more_records_exist() {
    let (app, _) = app();

    let response = app
        .oneshot(request("GET", "/customers?limit=1", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let body = body_json(response).await;

    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["paging"]["has_more"], true);
}

#[tokio::test]
async fn an_excessive_limit_is_clamped_rather_than_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(request(
            "GET",
            "/customers?limit=999999",
            json!({"tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        connector.last_query().1.limit,
        Some(1001),
        "clamped to max_limit plus the probe row"
    );
}

#[tokio::test]
async fn a_projection_reaches_the_connector() {
    let (app, connector) = app();

    app.oneshot(request(
        "GET",
        "/customers?select=id,name",
        json!({"tenant_id": "acme"}),
    ))
    .await
    .unwrap();

    assert_eq!(connector.last_query().1.fields.len(), 2);
}

#[tokio::test]
async fn a_descending_sort_reaches_the_connector() {
    let (app, connector) = app();

    app.oneshot(request(
        "GET",
        "/customers?sort=-id",
        json!({"tenant_id": "acme"}),
    ))
    .await
    .unwrap();

    let sort = connector.last_query().1.sort;

    assert_eq!(sort.len(), 1);
    assert_eq!(
        sort.first().unwrap().direction,
        fabric_connector::SortDirection::Descending
    );
}

#[tokio::test]
async fn an_invalid_field_name_in_a_filter_is_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(request(
            "GET",
            "/customers?drop%20table=1",
            json!({"tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_read_by_key_that_matches_nothing_is_a_404() {
    let (app, _) = empty_app();

    let response = app
        .oneshot(request("GET", "/customers/999", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_empty_list_is_an_empty_array_rather_than_an_error() {
    let (app, _) = empty_app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);
    assert_eq!(body["paging"]["has_more"], false);
}
