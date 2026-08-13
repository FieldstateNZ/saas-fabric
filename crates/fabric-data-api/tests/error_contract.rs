//! Items 13, 56, 57, 58, 59: the public error contract.
//!
//! Three things are asserted here that the other suites do not already
//! cover:
//!
//! 1. A masked internal error (500) carries a request id in its body, in an
//!    `X-Request-Id` response header, and in the tracing event that recorded
//!    the detail the caller did not get — so the id a caller quotes to an
//!    operator is the same id that finds the log line (item 57).
//! 2. A caller-supplied `X-Request-Id` is propagated, not replaced.
//! 3. An unknown-tenant probe (403) is its own event internally, even though
//!    externally it is indistinguishable from a disabled or
//!    never-provisioned tenant (items 13, 56, 58, 59).

mod support;

use axum::body::Body;
use fabric_data_api::API_PREFIX;
use fabric_identity::encode_unsigned_token;
use http::{Request, StatusCode};
use serde_json::json;
use support::{app, body_json, field_value, request};
use tower::ServiceExt as _;

#[tokio::test]
async fn a_server_error_carries_one_request_id_in_the_body_the_header_and_the_log() {
    let mut bindings = support::tenants();
    bindings.push(support::tenant_with_missing_data_source());

    let connector = support::RecordingConnector::new(vec![]);
    let app = support::app_with(
        support::resolver(bindings, support::data_sources()),
        connector,
        support::open_permissions(),
    );

    let (response, events) =
        support::capture(app.oneshot(request("GET", "/customers", json!({"tenant_id": "orphan"})))).await;
    let response = response.unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let header = response
        .headers()
        .get("x-request-id")
        .expect("a request id header")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(!header.is_empty());

    let body = body_json(response).await;
    assert_eq!(body["error"]["request_id"], header);

    let logged = field_value(&events, "data_api.request_failed", "request_id")
        .expect("the failure event to carry a request_id field");
    assert_eq!(logged, header);
}

#[tokio::test]
async fn an_inbound_request_id_is_echoed_back_unchanged() {
    let (app, _) = app();

    let serde_json::Value::Object(claims) = json!({"tenant_id": "acme"}) else {
        unreachable!("claims are always an object")
    };

    let request = Request::builder()
        .method("GET")
        .uri(format!("{API_PREFIX}/customers"))
        .header(
            "authorization",
            format!("Bearer {}", encode_unsigned_token(&claims)),
        )
        .header("x-request-id", "caller-supplied-id")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "caller-supplied-id"
    );
}

#[tokio::test]
async fn an_oversized_inbound_request_id_is_replaced_rather_than_echoed_or_trimmed() {
    // The id is reflected onto the header, into the error body, and into log
    // fields, so an unbounded one is a caller-controlled amplifier on all
    // three. A refused id is replaced outright: truncating would hand back
    // something that looks like the caller's id but no longer matches it.
    let (app, _) = app();

    let serde_json::Value::Object(claims) = json!({"tenant_id": "acme"}) else {
        unreachable!("claims are always an object")
    };
    let oversized = "a".repeat(1024 * 1024);

    let request = Request::builder()
        .method("GET")
        .uri(format!("{API_PREFIX}/customers"))
        .header(
            "authorization",
            format!("Bearer {}", encode_unsigned_token(&claims)),
        )
        .header("x-request-id", &oversized)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let echoed = response
        .headers()
        .get("x-request-id")
        .expect("a request id header")
        .to_str()
        .unwrap()
        .to_owned();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(uuid::Uuid::parse_str(&echoed).is_ok(), "a fresh id, not a trim");
    assert!(!oversized.starts_with(&echoed));
}

#[tokio::test]
async fn every_response_carries_a_request_id_header_even_on_success() {
    let (app, _) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-request-id").is_some());
}

#[tokio::test]
async fn an_unknown_tenant_probe_is_its_own_event_though_the_response_stays_generic() {
    let (app, connector) = app();

    let (response, events) =
        support::capture(app.oneshot(request("GET", "/customers", json!({"tenant_id": "ghost"})))).await;
    let response = response.unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(connector.query_count(), 0);

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "unknown_tenant");
    // Externally generic: the tenant is not named.
    assert!(!body["error"]["message"].as_str().unwrap().contains("ghost"));

    // Internally distinguishable: an operator can find this exact attempt.
    let tenant_logged = field_value(&events, "data_api.unknown_tenant_probed", "tenant_id")
        .expect("an internal event naming the probed tenant");
    assert!(tenant_logged.contains("ghost"));
}

#[tokio::test]
async fn an_unknown_tenant_and_a_scope_refusal_are_both_403_but_different_codes() {
    // Both fail closed the same way externally (§28) — same status — but the
    // `code` field is allowed to differ, because it says something about
    // *this request*, not about the tenant estate: "your token lacks a
    // scope" and "this tenant has nothing here" are different facts about
    // the caller, not a leak of which tenants exist.
    let unknown_tenant = {
        let (app, _) = app();
        app.oneshot(request("GET", "/customers", json!({"tenant_id": "ghost"})))
            .await
            .unwrap()
    };

    let scope_refused = {
        let connector = support::RecordingConnector::new(vec![]);
        let app = support::app_with(
            support::resolver(support::tenants(), support::data_sources()),
            connector,
            fabric_data_api::ResourcePermissions::default(),
        );
        app.oneshot(request(
            "DELETE",
            "/customers/1",
            json!({"tenant_id": "acme", "scope": "data:customers:read"}),
        ))
        .await
        .unwrap()
    };

    assert_eq!(unknown_tenant.status(), StatusCode::FORBIDDEN);
    assert_eq!(scope_refused.status(), StatusCode::FORBIDDEN);

    assert_eq!(body_json(unknown_tenant).await["error"]["code"], "unknown_tenant");
    assert_eq!(body_json(scope_refused).await["error"]["code"], "forbidden");
}

#[tokio::test]
async fn a_missing_data_source_and_an_unprimed_runtime_are_both_500_and_503_respectively_with_distinct_codes()
{
    // Item 13/56/58/59: two different platform-side failures must not
    // collapse into one signal, even though both are "the platform's
    // problem, not the caller's".
    let mut bindings = support::tenants();
    bindings.push(support::tenant_with_missing_data_source());

    let missing_data_source = {
        let connector = support::RecordingConnector::new(vec![]);
        let app = support::app_with(
            support::resolver(bindings, support::data_sources()),
            connector,
            support::open_permissions(),
        );
        app.oneshot(request("GET", "/customers", json!({"tenant_id": "orphan"})))
            .await
            .unwrap()
    };

    let unprimed_runtime = {
        let connector = support::RecordingConnector::new(vec![]);
        let runtime = std::sync::Arc::new(fabric_tenant_runtime::RuntimeResolver::new(
            std::sync::Arc::new(fabric_tenant_runtime::TenantRegistry::new()),
            std::sync::Arc::new(fabric_tenant_runtime::DataSourceRegistry::new()),
        ));
        let app = support::app_with(runtime, connector, support::open_permissions());
        app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
            .await
            .unwrap()
    };

    assert_eq!(missing_data_source.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(unprimed_runtime.status(), StatusCode::SERVICE_UNAVAILABLE);

    assert_eq!(body_json(missing_data_source).await["error"]["code"], "internal");
    assert_eq!(
        body_json(unprimed_runtime).await["error"]["code"],
        "runtime_unavailable"
    );
}
