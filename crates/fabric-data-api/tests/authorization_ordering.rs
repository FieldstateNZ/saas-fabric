//! Authorization is decided before any check that could describe the resource.
//!
//! The rule `prepare`'s rustdoc states, tested from outside: a caller who is
//! going to be refused must get the *same* refusal whatever they put in the
//! request, so status codes cannot be used to ask questions about a resource
//! they are not entitled to ask.
//!
//! Two leaks of the same shape are pinned shut here, because they were the
//! same defect found twice:
//!
//! - **The field allowlist.** A 400 "unknown field salary" arriving ahead of a
//!   403 let an unauthorised caller enumerate a resource's `queryable_fields`
//!   one guess at a time.
//! - **The operations list.** A 405 "delete is not available on
//!   readOnlyReport" arriving ahead of a 403 let the same caller enumerate
//!   every catalogue entry's `operations` list one verb at a time.
//!
//! Both are global catalogue data rather than tenant data, so the severity was
//! low; both describe what a resource *can do*, which is a fact for callers
//! who already have access to it. The second survived the pass that fixed the
//! first, so this suite covers the rule rather than either instance of it.
//!
//! Field-list tests use `restrictedCustomers`, the one catalogue fixture with
//! a non-empty `queryable_fields`. Operations-list tests use `customers`
//! against `readOnlyReport`, which are identical fixtures apart from the verbs
//! they expose. All of them use `ResourcePermissions::default()`, so scope
//! checks are actually enforced.

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

/// A token for a named tenant allowed to write one named resource.
fn write_scopes(tenant: &str, resource: &str) -> Value {
    json!({"tenant_id": tenant, "scope": format!("data:{resource}:write")})
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

/// The resource whose catalogue entry exposes every verb.
const OPEN_RESOURCE: &str = "customers";

/// The resource catalogued without the write verbs.
///
/// Identical to [`OPEN_RESOURCE`] in every other respect — same logical data
/// source, same collection, same (empty) field allowlist — so any difference
/// a caller can observe between the two is the `operations` list and nothing
/// else.
const CLOSED_RESOURCE: &str = "readOnlyReport";

/// The verbs [`CLOSED_RESOURCE`] withholds, with the request shape each uses.
///
/// `read` and `list` are absent because both resources expose them, so they
/// could not tell the two apart whatever the ordering.
const WITHHELD_VERBS: [(&str, &str); 3] = [("POST", ""), ("PATCH", "/1"), ("DELETE", "/1")];

/// A fixture whose tenant sits on a DataSource the platform refuses writes to,
/// so [`DataApiError::ResourceIsReadOnly`] — the *other* 405 — is reachable.
fn replica_app() -> (Router, Arc<RecordingConnector>) {
    let connector = RecordingConnector::new(vec![support::row(1, "Alice")]);
    let runtime = support::resolver(
        vec![support::tenant_on_replica()],
        vec![support::read_only_data_source()],
    );

    (
        support::app_with(runtime, Arc::clone(&connector), ResourcePermissions::default()),
        connector,
    )
}

/// Issues a write against a named resource and reduces the response, replacing
/// the resource name inside the message with a fixed placeholder.
///
/// Every refusal echoes back the name the caller supplied, so comparing two
/// resources' responses raw would always differ and prove nothing. Normalising
/// only that one caller-controlled substring leaves exactly the difference
/// these tests are looking for: whether the refusal itself changes shape.
async fn write(
    app: &Router,
    method: &str,
    suffix: &str,
    resource: &str,
    claims: Value,
) -> (StatusCode, Value) {
    let uri = format!("/{resource}{suffix}");

    let built = if method == "DELETE" {
        request(method, &uri, claims)
    } else {
        json_request(method, &uri, claims, &json!({"name": "Alice"}))
    };

    let (status, mut body) = observable(app.clone().oneshot(built).await.unwrap()).await;

    if let Some(Value::String(message)) = body.pointer_mut("/error/message") {
        *message = message.replace(resource, "{resource}");
    }

    (status, body)
}

#[tokio::test]
async fn an_unauthorised_caller_cannot_tell_an_exposed_verb_from_a_withheld_one() {
    // The defect this suite was extended for. `resource.allows(operation)` ran
    // at step 2 and `authorize` at step 3, so a caller holding zero scopes got
    // 403 "not permitted to delete customers" for a verb the catalogue exposes
    // and 405 "delete is not available on readOnlyReport" for one it withholds
    // — enumerating every catalogue entry's `operations` list a verb at a time.
    let (app, connector) = scoped_app();

    // Collected rather than asserted verb by verb, so a failure prints the
    // whole oracle at once instead of stopping at the first verb of three.
    let mut exposed = Vec::new();
    let mut withheld = Vec::new();

    for (method, suffix) in WITHHELD_VERBS {
        exposed.push((
            method,
            write(&app, method, suffix, OPEN_RESOURCE, no_scopes()).await,
        ));
        withheld.push((
            method,
            write(&app, method, suffix, CLOSED_RESOURCE, no_scopes()).await,
        ));
    }

    assert!(exposed
        .iter()
        .all(|(_, (status, _))| *status == StatusCode::FORBIDDEN));
    assert_eq!(
        exposed, withheld,
        "no verb may reveal whether the catalogue exposes it"
    );
    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn an_authorised_caller_still_learns_a_verb_is_unavailable() {
    // The half of the fix that is easy to lose: 405 has to survive for a
    // caller entitled to it. Someone holding write scope on `readOnlyReport`
    // is entitled to know the catalogue exposes no writes there, and
    // collapsing that into 403 would tell them their token is wrong when it is
    // the resource that cannot do it — the least diagnosable answer available.
    let (app, connector) = scoped_app();

    for (method, suffix) in WITHHELD_VERBS {
        let claims = write_scopes("acme", CLOSED_RESOURCE);
        let (status, body) = write(&app, method, suffix, CLOSED_RESOURCE, claims).await;

        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "`{method}` must still 405"
        );
        assert_eq!(body["error"]["code"], "operation_not_allowed");
    }

    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn an_authorised_caller_still_reaches_a_verb_the_catalogue_exposes() {
    // And the reorder must not refuse anything it used to allow.
    let (app, connector) = scoped_app();

    for (method, suffix) in WITHHELD_VERBS {
        let claims = write_scopes("acme", OPEN_RESOURCE);
        let (status, _) = write(&app, method, suffix, OPEN_RESOURCE, claims).await;

        assert!(status.is_success(), "`{method}` must still execute, got {status}");
    }

    assert_eq!(connector.mutation_count(), WITHHELD_VERBS.len());
}

#[tokio::test]
async fn an_unauthorised_caller_cannot_learn_the_tenant_is_placed_on_a_read_only_data_source() {
    // The other 405. It was already behind authorization — it needs the
    // resolved DataSource, which `prepare` only reaches after authorising — but
    // "already correct" is precisely what step 2 looked like, so it is pinned
    // rather than assumed. This one describes the tenant's *placement*, which
    // is further from the caller than the catalogue and internal by §2.
    let (app, connector) = replica_app();

    let claims = json!({"tenant_id": "reader"});
    let (status, body) = write(&app, "DELETE", "/1", OPEN_RESOURCE, claims).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn an_authorised_caller_still_learns_the_data_source_is_read_only() {
    let (app, connector) = replica_app();

    let claims = write_scopes("reader", OPEN_RESOURCE);
    let (status, body) = write(&app, "DELETE", "/1", OPEN_RESOURCE, claims).await;

    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body["error"]["code"], "read_only");
    assert_eq!(connector.mutation_count(), 0);
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
