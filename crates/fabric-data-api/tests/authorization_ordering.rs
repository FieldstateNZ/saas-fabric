//! Authorization is decided before any check that could describe the resource.
//!
//! The rule `prepare`'s rustdoc states, tested from outside: a caller who is
//! going to be refused must get the *same* refusal whatever they put in the
//! request, so status codes cannot be used to ask questions about a resource
//! they are not entitled to ask.
//!
//! The specific leak this suite pins shut: `queryable_fields` is global
//! catalogue data, not tenant data, so the severity was low — but a 400
//! "unknown field salary" arriving ahead of a 403 let an unauthorised caller
//! enumerate a resource's field allowlist one guess at a time.
//!
//! Every test uses `restrictedCustomers`, the one catalogue fixture with a
//! non-empty `queryable_fields`, and `ResourcePermissions::default()`, so
//! scope checks are actually enforced.

mod support;

use std::sync::Arc;

use axum::Router;
use fabric_data_api::{DataApiConfig, ResourcePermissions};
use http::StatusCode;
use serde_json::{json, Value};
use support::{body_json, json_request, request, RecordingConnector};
use tower::ServiceExt as _;

/// The resource with a field allowlist, so "this field exists" is a fact worth
/// protecting rather than a tautology.
const RESOURCE: &str = "/restrictedCustomers";

/// The standard fixture with scope checks on.
fn scoped_app() -> (Router, Arc<RecordingConnector>) {
    scoped_app_with(&DataApiConfig::default())
}

/// The same, with a caller-chosen configuration.
fn scoped_app_with(config: &DataApiConfig) -> (Router, Arc<RecordingConnector>) {
    let connector = RecordingConnector::new(vec![support::row(1, "Alice")]);
    let runtime = support::resolver(support::tenants(), support::data_sources());

    (
        support::app_with_config(
            runtime,
            Arc::clone(&connector) as Arc<dyn fabric_connector::DataConnector>,
            ResourcePermissions::default(),
            config,
        ),
        connector,
    )
}

/// A token for `acme` carrying no scopes at all.
fn no_scopes() -> Value {
    json!({"tenant_id": "acme"})
}

/// A token allowed to read and write `restrictedCustomers`.
fn full_scopes() -> Value {
    json!({
        "tenant_id": "acme",
        "scope": "data:restrictedCustomers:read data:restrictedCustomers:write"
    })
}

/// A response reduced to what a caller can actually compare between two
/// requests: the status, and the body with the per-request correlation id
/// removed. Two requests that agree here are indistinguishable, which is the
/// property every test below asserts.
async fn observable(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let mut body = body_json(response).await;

    if let Some(error) = body.get_mut("error").and_then(Value::as_object_mut) {
        error.remove("request_id");
    }

    (status, body)
}

/// Issues a `GET` and reduces the response.
async fn get(app: &Router, query: &str, claims: Value) -> (StatusCode, Value) {
    let uri = format!("{RESOURCE}{query}");
    let response = app.clone().oneshot(request("GET", &uri, claims)).await.unwrap();

    observable(response).await
}

/// Issues a request carrying a JSON body and reduces the response.
async fn send(app: &Router, method: &str, uri: &str, claims: Value, body: &Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(json_request(method, uri, claims, body))
        .await
        .unwrap();

    observable(response).await
}

#[tokio::test]
async fn an_unauthorised_caller_cannot_tell_a_real_field_from_an_invented_one() {
    // The defect itself. Before the fix, `name` reached authorization and
    // answered 403 while `salary` was rejected at parse time with a 400 that
    // named it — so a caller with no scopes could walk the field list.
    let (app, connector) = scoped_app();

    let real = get(&app, "?name=Alice", no_scopes()).await;
    let hidden = get(&app, "?salary=100000", no_scopes()).await;
    let invented = get(&app, "?no_such_field_anywhere=1", no_scopes()).await;

    assert_eq!(real.0, StatusCode::FORBIDDEN);
    assert_eq!(real, hidden, "a hidden field must answer like a permitted one");
    assert_eq!(real, invented, "an invented field must answer like a real one");
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn every_way_of_naming_a_field_answers_the_same_refusal() {
    // Filters are not the only channel: `sort` and `select` run the same
    // `permits_field` check, and a name that is not an identifier at all is
    // rejected by the same parse. All of it now sits behind authorization, so
    // all of it answers identically.
    let (app, connector) = scoped_app();

    let baseline = get(&app, "?name=Alice", no_scopes()).await;

    for query in [
        "?sort=name",
        "?sort=salary",
        "?sort=-no_such_field",
        "?select=name",
        "?select=salary",
        "?drop%20table=1",
        "?limit=not-a-number",
        "",
    ] {
        assert_eq!(
            get(&app, query, no_scopes()).await,
            baseline,
            "`{query}` must be indistinguishable from a plain refusal"
        );
    }

    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn an_authorised_caller_still_gets_the_field_check() {
    // The other half of the fix: moving the parse must not disable it. A
    // caller who *is* allowed to list still cannot reach a hidden column.
    let (app, connector) = scoped_app();

    let (refused, body) = get(&app, "?salary=100000", full_scopes()).await;
    assert_eq!(refused, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["message"], "unknown field salary");

    let (permitted, _) = get(&app, "?name=Alice", full_scopes()).await;
    assert_eq!(permitted, StatusCode::OK);

    assert_eq!(connector.query_count(), 1, "only the permitted query executes");
}

#[tokio::test]
async fn an_unauthorised_write_cannot_tell_a_real_field_from_an_invented_one() {
    // Writes validate field names too, in `row_mapping::to_row`. They already
    // ran after authorization; this pins that so a future refactor of the
    // handlers cannot quietly reintroduce the list-side defect on this path.
    let (app, connector) = scoped_app();

    let real = send(&app, "POST", RESOURCE, no_scopes(), &json!({"name": "Alice"})).await;
    let hidden = send(&app, "POST", RESOURCE, no_scopes(), &json!({"salary": 100_000})).await;

    assert_eq!(real.0, StatusCode::FORBIDDEN);
    assert_eq!(real, hidden);
    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn an_unauthorised_update_cannot_tell_a_real_field_from_an_invented_one() {
    let (app, connector) = scoped_app();
    let uri = format!("{RESOURCE}/1");

    let real = send(&app, "PATCH", &uri, no_scopes(), &json!({"name": "Alice"})).await;
    let hidden = send(&app, "PATCH", &uri, no_scopes(), &json!({"salary": 100_000})).await;

    assert_eq!(real.0, StatusCode::FORBIDDEN);
    assert_eq!(real, hidden);
    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn a_request_limit_is_not_reported_to_a_caller_who_may_not_make_the_request() {
    // Limits could safely be checked first — every bound is a deployment-wide
    // constant that describes the API, not the resource. They are checked
    // after authorization anyway, because they operate on the parsed query and
    // that parse had to move behind it. Keeping them together costs nothing
    // and means there is one rule to remember rather than two.
    let config = DataApiConfig {
        max_filters: 1,
        ..DataApiConfig::default()
    };
    let (app, connector) = scoped_app_with(&config);

    let (refused, body) = get(&app, "?name=a&id=b", no_scopes()).await;
    assert_eq!(refused, StatusCode::FORBIDDEN);
    assert!(!body.to_string().contains("too many"));

    // The bound is still enforced for a caller entitled to be told about it.
    let (over, message) = get(&app, "?name=a&id=b", full_scopes()).await;
    assert_eq!(over, StatusCode::BAD_REQUEST);
    assert_eq!(message["error"]["message"], "too many filters: at most 1 allowed");

    assert_eq!(connector.query_count(), 0);
}
