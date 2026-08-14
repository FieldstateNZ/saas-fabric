//! What a caller is told when the call to the backend itself broke.
//!
//! # The defect this pins
//!
//! Every transport failure used to collapse into one answer: `503`, code
//! `execution_failed`, "the request could not be executed". A `POST` carries up
//! to 500 rows and no idempotency key, and 503 is the status meshes and SDKs
//! retry on their own — so a mutation the backend **received and applied** was
//! reported as a safe retry, and something downstream took it.
//!
//! `ConnectorError` now distinguishes three positions in the HTTP exchange, and
//! this suite drives each of them through the assembled router for both a read
//! and a write. The distinction only matters for the write: a read changed
//! nothing whatever happened on the wire, so all three stay retryable there.

mod support;

use axum::body::Body;
use axum::response::IntoResponse as _;
use axum::Router;
use fabric_connector::ConnectorError;
use fabric_data_api::DataApiConfig;
use http::{Request, StatusCode};
use serde_json::{json, Value};
use support::{
    app_with_config, body_json, data_sources, json_request, malformed_response, open_permissions,
    outcome_unknown, rejected, rejected_outright, request, resolver, result_lost, tenants, unreachable,
    ScriptedConnector,
};
use tower::ServiceExt as _;

/// The assembled router over a connector that fails every operation this way.
fn app(build: fn() -> ConnectorError) -> Router {
    app_with_config(
        resolver(tenants(), data_sources()),
        ScriptedConnector::failing(build),
        open_permissions(),
        &DataApiConfig::default(),
    )
}

fn acme() -> Value {
    json!({"tenant_id": "acme"})
}

fn read() -> Request<Body> {
    request("GET", "/customers", acme())
}

fn write() -> Request<Body> {
    json_request("POST", "/customers", acme(), &json!({"name": "Alice"}))
}

/// The status, the machine code, the message, and whether the platform told the
/// caller to come back — the four things a client branches on.
struct Answer {
    status: StatusCode,
    code: String,
    message: String,
    retry_after: bool,
}

/// Reads one string out of the error envelope, or the empty string.
///
/// `get` rather than indexing because this runs outside a `#[test]` body, where
/// the panicking forms are denied — the same rule the crate itself follows.
fn envelope(body: &Value, name: &str) -> String {
    body.get("error")
        .and_then(|error| error.get(name))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

async fn answer(build: fn() -> ConnectorError, request: Request<Body>) -> Answer {
    // `Router::oneshot` is infallible; the fallback exists only because its
    // signature says `Result`, and a failed request would fail every assertion
    // below anyway.
    let response = app(build)
        .oneshot(request)
        .await
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());

    let status = response.status();
    let retry_after = response.headers().contains_key("retry-after");
    let body = body_json(response).await;

    Answer {
        status,
        code: envelope(&body, "code"),
        message: envelope(&body, "message"),
        retry_after,
    }
}

// -- Reads: nothing was mutated, so every position on the wire is retryable ---

#[tokio::test]
async fn a_read_that_never_reached_the_backend_is_a_retryable_503() {
    let answer = answer(unreachable, read()).await;

    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer.code, "connector_unavailable");
    assert!(answer.retry_after, "a 503 the platform invites a retry on");
}

#[tokio::test]
async fn a_read_whose_outcome_is_unknown_is_still_a_retryable_503() {
    // The asymmetry with the write path: for a read there is no outcome worth
    // being unsure about, because nothing was changed either way.
    let answer = answer(outcome_unknown, read()).await;

    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer.code, "connector_unavailable");
    assert!(answer.retry_after);
}

#[tokio::test]
async fn a_read_whose_result_was_lost_is_still_a_retryable_503() {
    let answer = answer(result_lost, read()).await;

    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer.code, "connector_unavailable");
}

// -- Writes: the three positions mean three different things ------------------

#[tokio::test]
async fn a_write_that_never_reached_the_backend_is_the_one_retryable_answer() {
    let answer = answer(unreachable, write()).await;

    assert_eq!(answer.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer.code, "connector_unavailable");
    assert!(answer.retry_after, "the only write failure that invites a retry");
    assert!(answer.message.contains("not carried out"), "{}", answer.message);
    assert!(answer.message.contains("safe to retry"), "{}", answer.message);
}

#[tokio::test]
async fn a_write_with_no_answer_is_a_502_the_caller_must_reconcile() {
    let answer = answer(outcome_unknown, write()).await;

    assert_eq!(answer.status, StatusCode::BAD_GATEWAY);
    assert_eq!(answer.code, "write_outcome_unknown");
    assert!(
        !answer.retry_after,
        "the platform must not instruct a retry it cannot make safe"
    );
    assert!(answer.message.contains("may or may not"), "{}", answer.message);
    assert!(answer.message.contains("not idempotent"), "{}", answer.message);
}

#[tokio::test]
async fn a_write_whose_result_was_lost_says_plainly_that_it_was_applied() {
    // The sharpest case: the backend answered with a success status, so the
    // rows are in. Only the affected count is gone.
    let answer = answer(result_lost, write()).await;

    assert_eq!(answer.status, StatusCode::BAD_GATEWAY);
    assert_eq!(answer.code, "write_result_unavailable");
    assert!(!answer.retry_after);
    assert!(answer.message.contains("was applied"), "{}", answer.message);
    assert!(answer.message.contains("not retry"), "{}", answer.message);
}

#[tokio::test]
async fn the_three_write_answers_do_not_collapse_into_one_code() {
    // The regression itself: clients branch on `code`, and these three need
    // three different client behaviours.
    let not_applied = answer(unreachable, write()).await;
    let unknown = answer(outcome_unknown, write()).await;
    let applied = answer(result_lost, write()).await;

    assert_ne!(not_applied.code, unknown.code);
    assert_ne!(unknown.code, applied.code);
    assert_ne!(not_applied.code, applied.code);
}

#[tokio::test]
async fn a_patch_gets_the_write_mapping_not_the_read_one() {
    let response = app(outcome_unknown)
        .oneshot(json_request(
            "PATCH",
            "/customers/1",
            acme(),
            &json!({"name": "Renamed"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "write_outcome_unknown"
    );
}

#[tokio::test]
async fn a_delete_gets_the_write_mapping_not_the_read_one() {
    let response = app(result_lost)
        .oneshot(request("DELETE", "/customers/1", acme()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "write_result_unavailable"
    );
}

// -- The two failures that answer after a success status ----------------------

#[tokio::test]
async fn a_malformed_response_to_a_write_does_not_claim_the_write_failed() {
    // `effect()` classifies it `Applied`: it is only ever built after a 2xx.
    // It stays a 500 — a version skew an operator fixes — but the message may
    // not tell the caller their rows are absent.
    let answer = answer(malformed_response, write()).await;

    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(answer.message.contains("carried out"), "{}", answer.message);
    assert!(answer.message.contains("not retry"), "{}", answer.message);
}

#[tokio::test]
async fn a_write_refused_mid_flight_does_not_claim_it_did_not_happen() {
    // A 409, whose specification example is a foreign key constraint the data
    // source raises while writing. It is 4xx and it is still not conclusive:
    // nothing makes a single opaque procedure atomic, so rows may be in.
    let answer = answer(rejected, write()).await;

    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        answer.message.contains("read the current state"),
        "{}",
        answer.message
    );
}

#[tokio::test]
async fn a_write_the_backend_would_not_accept_is_reported_as_not_carried_out() {
    // The other direction, end to end. A 400 means the connector declined the
    // request rather than failing part-way through it, so the platform can tell
    // the caller their records are untouched instead of sending them to look.
    let answer = answer(rejected_outright, write()).await;

    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        answer.message.contains("was not carried out"),
        "{}",
        answer.message
    );
    // Still masked: the connector's text names a schema and a column.
    assert!(!answer.message.contains("acme_prod"), "{}", answer.message);
}

#[tokio::test]
async fn a_malformed_response_to_a_read_stays_masked() {
    // Nothing was changed, so there is nothing to be honest about beyond the
    // mask — and the detail names tables and servers.
    let answer = answer(malformed_response, read()).await;

    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(answer.message, "internal error");
}

// -- A more informative status must not become a more informative message -----

#[tokio::test]
async fn no_transport_answer_names_the_infrastructure_that_failed() {
    for build in [
        unreachable,
        outcome_unknown,
        result_lost,
        rejected,
        malformed_response,
    ] {
        for request in [read(), write()] {
            let answer = answer(build, request).await;

            for internal in ["sql-au-east-03", "5432", "acme_prod", "postgres"] {
                assert!(
                    !answer.message.contains(internal),
                    "{internal:?} leaked into: {}",
                    answer.message
                );
            }
        }
    }
}
