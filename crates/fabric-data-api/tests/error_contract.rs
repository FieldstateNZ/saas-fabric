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
//! 4. The one error arm that repeats anything a connector wrote repeats only
//!    neutral capability names, keeps the refusal's physical detail out of the
//!    body, and records every connector refusal whether it answers 4xx or 5xx.

mod support;

use axum::body::Body;
use fabric_connector::{ComparisonOperator, UnsupportedFeature};
use fabric_data_api::API_PREFIX;
use fabric_identity::encode_unsigned_token;
use http::{Request, StatusCode};
use serde_json::json;
use support::{app, body_json, field_value, json_request, request};
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

    // Built by hand rather than through `request`, so the registered issuer
    // the resolver binds on has to be written out here.
    let serde_json::Value::Object(claims) = json!({"iss": support::issuer_for("acme"), "tenant_id": "acme"})
    else {
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

/// The assembled router over a connector that refuses everything as
/// unsupported, naming the given capability.
fn refusing_app(feature: UnsupportedFeature) -> axum::Router {
    app_over(support::ScriptedConnector::refusing(feature))
}

/// The same, over a refusal that also carries the physical detail a
/// translating connector records for an operator.
fn refusing_app_with_detail(feature: UnsupportedFeature, detail: &str) -> axum::Router {
    app_over(support::ScriptedConnector::refusing_with_detail(feature, detail))
}

fn app_over(connector: std::sync::Arc<support::ScriptedConnector>) -> axum::Router {
    support::app_with_config(
        support::resolver(support::tenants(), support::data_sources()),
        connector,
        support::open_permissions(),
        &fabric_data_api::DataApiConfig::default(),
    )
}

#[tokio::test]
async fn a_refusals_physical_detail_never_reaches_the_body() {
    // Reachable output before the type change: `feature` was a `String`, and a
    // translating connector interpolated whatever it was translating. The
    // discriminator predicate has already been conjoined by then, so a 400 read
    // `comparing customer_records_v2.tenant_key with a Equal operator` — naming
    // the shared table and the column holding the tenant boundary up.
    //
    // Those identifiers can no longer go in `feature` at all; they go in the
    // refusal's detail. This asserts that half stays internal while the caller
    // still learns which comparison to change.
    let app = refusing_app_with_detail(
        UnsupportedFeature::Comparison(ComparisonOperator::Equal),
        "customer_records_v2.tenant_key has no equal operator in the connector schema",
    );

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = body_json(response).await;

    assert_eq!(body["error"]["code"], "unsupported");
    assert_eq!(
        body["error"]["message"],
        "this operation is not supported: the equal comparison"
    );

    let full = body.to_string();
    assert!(!full.contains("customer_records_v2"));
    assert!(!full.contains("tenant_key"));
}

#[tokio::test]
async fn a_refused_write_names_the_capability_not_the_procedure_mapping() {
    let app = refusing_app_with_detail(
        UnsupportedFeature::WritesToCollection,
        "no procedure mapping is configured for customer_records_v2",
    );

    let response = app
        .oneshot(json_request(
            "POST",
            "/customers",
            json!({"tenant_id": "globex"}),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let full = body_json(response).await.to_string();
    assert!(!full.contains("customer_records_v2"));
    assert!(!full.contains("procedure"));
}

#[tokio::test]
async fn a_neutral_capability_name_is_still_explained_to_the_caller() {
    // The reason this is an allowlist rather than a blanket mask: an
    // authorised caller who asked for a sort the backend cannot do is
    // entitled to know it was the sort they should change.
    let app = refusing_app(UnsupportedFeature::Ordering);

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(response).await["error"]["message"],
        "this operation is not supported: ordering"
    );
}

#[tokio::test]
async fn a_masked_connector_refusal_is_recorded_even_though_it_is_only_a_400() {
    // The observability half. The caller gets a replaced message, so if the
    // connector's own words are not recorded here they are lost — and a
    // 400-class connector failure is a catalogue-versus-backend drift that
    // only an operator can fix.
    let app = refusing_app_with_detail(
        UnsupportedFeature::Comparison(ComparisonOperator::Equal),
        "customer_records_v2.tenant_key has no equal operator in the connector schema",
    );

    let (response, events) =
        support::capture(app.oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))).await;
    let response = response.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let request_id = response
        .headers()
        .get("x-request-id")
        .expect("a request id header")
        .to_str()
        .unwrap()
        .to_owned();

    let detail = field_value(&events, "data_api.connector_refused", "detail")
        .expect("a 400-class connector refusal to be recorded");
    assert!(detail.contains("customer_records_v2"));

    // Same id the caller was given, so a report that quotes it finds this line.
    let logged_id = field_value(&events, "data_api.connector_refused", "request_id")
        .expect("the refusal event to carry a request_id");
    assert_eq!(logged_id, request_id);
}

#[tokio::test]
async fn a_connector_refusal_is_recorded_once_on_the_4xx_event_and_not_also_as_a_failure() {
    // The two events partition connector failures by status. A 4xx must not
    // land on `request_failed` as well, or every masked refusal is counted
    // twice and the error-rate signal an operator alerts on is wrong.
    let app = refusing_app(UnsupportedFeature::Ordering);

    let (response, events) =
        support::capture(app.oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))).await;

    assert_eq!(response.unwrap().status(), StatusCode::BAD_REQUEST);
    assert!(field_value(&events, "data_api.request_failed", "detail").is_none());
    assert!(field_value(&events, "data_api.connector_refused", "detail").is_some());
}

#[tokio::test]
async fn a_server_side_connector_failure_stays_on_the_existing_failure_event() {
    // The other half of the partition: a 5xx keeps `request_failed`, so the
    // new arm did not quietly capture cases that already had a home.
    let mut bindings = support::tenants();
    bindings.push(support::tenant_with_missing_data_source());

    let app = support::app_with(
        support::resolver(bindings, support::data_sources()),
        support::RecordingConnector::new(vec![]),
        support::open_permissions(),
    );

    let (response, events) =
        support::capture(app.oneshot(request("GET", "/customers", json!({"tenant_id": "orphan"})))).await;

    assert_eq!(response.unwrap().status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(field_value(&events, "data_api.request_failed", "detail").is_some());
    assert!(field_value(&events, "data_api.connector_refused", "detail").is_none());
}
