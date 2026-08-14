//! What a write is allowed to claim, driven through the assembled router.
//!
//! The rule under test is narrow and deliberate: the platform knows how many
//! rows it sent, it knows what number came back, and it may report success only
//! when those agree. It cannot know whether a batch was applied atomically, and
//! it cannot know which rows landed — see `execution::write_integrity` for why
//! neither is obtainable from any connector capability.
//!
//! The failure this exists to prevent is a `201 Created` carrying a count that
//! silently disagrees with the request. That is worse than an error: the caller
//! learns a number and nothing else — not which rows, not that anything failed.

mod support;

use std::sync::Arc;

use axum::Router;
use fabric_data_api::DataApiConfig;
use http::StatusCode;
use serde_json::{json, Value};
use support::{
    app_with_config, body_json, data_sources, json_request, open_permissions, request, resolver, tenants,
    wide_row, CountingConnector,
};
use tower::ServiceExt as _;

fn app(connector: &Arc<CountingConnector>) -> Router {
    app_with_config(
        resolver(tenants(), data_sources()),
        Arc::clone(connector) as _,
        open_permissions(),
        &DataApiConfig::default(),
    )
}

fn acme() -> Value {
    json!({"tenant_id": "acme"})
}

fn five_rows() -> Value {
    json!([
        {"name": "Alice"},
        {"name": "Bob"},
        {"name": "Carol"},
        {"name": "Dave"},
        {"name": "Erin"},
    ])
}

#[tokio::test]
async fn a_batch_the_backend_only_partly_applied_is_not_created() {
    // The reported defect: five rows in, three applied, `201 Created`.
    let connector = CountingConnector::reporting(3);

    let response = app(&connector)
        .oneshot(json_request("POST", "/customers", acme(), &five_rows()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "partial_write");
}

#[tokio::test]
async fn a_partial_write_tells_the_caller_the_counts_and_admits_what_it_cannot_say() {
    let connector = CountingConnector::reporting(3);

    let response = app(&connector)
        .oneshot(json_request("POST", "/customers", acme(), &five_rows()))
        .await
        .unwrap();

    let body = body_json(response).await;
    let message = body["error"]["message"].as_str().unwrap();

    // The counts describe the caller's own request, so they are safe to send.
    assert!(message.contains('3') && message.contains('5'), "got: {message}");
    // And the platform says plainly that it cannot identify the rows.
    assert!(message.contains("cannot determine which"), "got: {message}");
}

#[tokio::test]
async fn a_partial_write_names_nothing_physical() {
    let connector = CountingConnector::reporting(1);

    let response = app(&connector)
        .oneshot(json_request("POST", "/customers", acme(), &five_rows()))
        .await
        .unwrap();

    let message = body_json(response).await["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned();

    for internal in ["postgres", "acme-prod", "customers", "primary", "tenant_key"] {
        assert!(!message.contains(internal), "{internal:?} leaked into: {message}");
    }
}

#[tokio::test]
async fn a_single_row_insert_that_applied_nothing_is_a_partial_write_too() {
    // Why refusing multi-row batches would not have closed the defect: a
    // connector whose procedure yields `null` reports zero for a one-row
    // insert, and that was `201 Created` as well.
    let connector = CountingConnector::reporting(0);

    let response = app(&connector)
        .oneshot(json_request(
            "POST",
            "/customers",
            acme(),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body_json(response).await["error"]["code"], "partial_write");
}

#[tokio::test]
async fn a_batch_the_backend_fully_applied_is_created() {
    let connector = CountingConnector::reporting(5);

    let response = app(&connector)
        .oneshot(json_request("POST", "/customers", acme(), &five_rows()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(body_json(response).await["affected"], 5);
}

#[tokio::test]
async fn a_successful_write_reports_exactly_what_was_sent() {
    // `affected` is now a checked number rather than a relayed one.
    let connector = CountingConnector::reporting(1);

    let response = app(&connector)
        .oneshot(json_request(
            "POST",
            "/customers",
            acme(),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(body_json(response).await["affected"], 1);
}

#[tokio::test]
async fn a_count_larger_than_the_request_is_refused_as_malformed() {
    // The reviewer's second case: `{"affected":500}` for a one-row request.
    // Not a partial write — an incoherent answer, so nothing else the
    // connector said about this operation is trusted either.
    let connector = CountingConnector::reporting(500);

    let response = app(&connector)
        .oneshot(json_request(
            "POST",
            "/customers",
            acme(),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "execution_failed");

    let message = body["error"]["message"].as_str().unwrap();
    // The impossible count is for the log, not for the caller: it would be
    // describing rows that are not theirs.
    assert!(!message.contains("500"), "{message}");
    // But the caller is not told their write is absent. A `MalformedResponse`
    // is only ever built after a success, this one included — the mutation ran,
    // and the number it came back with is the only thing that is unusable.
    assert!(message.contains("was carried out"), "{message}");
}

#[tokio::test]
async fn an_inflated_count_cannot_reach_the_caller_as_a_success() {
    let connector = CountingConnector::reporting(500);

    let response = app(&connector)
        .oneshot(json_request("POST", "/customers", acme(), &five_rows()))
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::CREATED);
    assert!(body_json(response).await.get("affected").is_none());
}

#[tokio::test]
async fn rows_the_backend_returned_still_reach_the_caller_on_success() {
    // The one channel through which a caller can tell *what* landed. It is the
    // connector's choice whether to populate it, which is why the partial case
    // says the platform cannot determine the rows rather than promising them.
    let connector = CountingConnector::reporting_with_rows(1, vec![wide_row(7, "Alice")]);

    let response = app(&connector)
        .oneshot(json_request(
            "POST",
            "/customers",
            acme(),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(body_json(response).await["data"][0]["id"], 7);
}

#[tokio::test]
async fn a_keyed_update_matching_nothing_is_not_a_partial_write() {
    // Zero is a legitimate outcome for a predicate: the key matched no record.
    // The platform sent a filter, not rows, so there is no shortfall to report.
    let connector = CountingConnector::reporting(0);

    let response = app(&connector)
        .oneshot(json_request(
            "PATCH",
            "/customers/1",
            acme(),
            &json!({"name": "Renamed"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["affected"], 0);
}

#[tokio::test]
async fn a_keyed_delete_claiming_several_records_is_refused() {
    // A delete addressed by key cannot honestly reach five rows. Under
    // discriminator isolation that is the shape of the worst failure there is,
    // so it is refused rather than reported as a successful bulk delete.
    let connector = CountingConnector::reporting(5);

    let response = app(&connector)
        .oneshot(request("DELETE", "/customers/1", acme()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body_json(response).await["error"]["code"], "execution_failed");
}

#[tokio::test]
async fn a_keyed_delete_of_its_one_record_succeeds() {
    let connector = CountingConnector::reporting(1);

    let response = app(&connector)
        .oneshot(request("DELETE", "/customers/1", acme()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["affected"], 1);
}
