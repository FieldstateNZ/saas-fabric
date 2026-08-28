//! `queryable_fields` gates what comes back, not only what may be asked for.
//!
//! Three earlier passes hardened the allowlist against enumeration through
//! `select`, filters, and sort. None of them gated the *default* projection,
//! so the control refused `?select=salary` with a 400 and then returned
//! `salary` in the body of the very next request. The same hole existed on
//! `returned_rows`, so a connector implementing `RETURNING` disclosed it on a
//! `POST` as well.
//!
//! Every test here drives the assembled router, because that is the only level
//! at which "what does a caller actually receive" is a fact rather than an
//! assumption. `globex` is used throughout: it is the fixture tenant on the
//! shared DataSource, so its rows carry `tenant_key` — the discriminator column
//! and the tenant's internal surrogate, which §26 says an application must
//! never see.

mod support;

use std::sync::Arc;

use fabric_tenant_runtime::RuntimeResolver;
use http::StatusCode;
use serde_json::{json, Value};
use support::{
    app_with_config, body_json, data_sources, json_request, open_permissions, request, resolver, tenants,
    wide_row, ScriptedConnector,
};
use tower::ServiceExt as _;

/// The standard runtime, with a connector that returns every column.
fn runtime() -> Arc<RuntimeResolver> {
    resolver(tenants(), data_sources())
}

/// The assembled router over a connector returning one wide row.
fn app() -> axum::Router {
    app_with_config(
        runtime(),
        ScriptedConnector::returning(vec![wide_row(1, "Alice")]),
        open_permissions(),
        &fabric_data_api::DataApiConfig::default(),
    )
}

fn globex() -> Value {
    json!({"tenant_id": "globex"})
}

#[tokio::test]
async fn a_list_with_no_select_returns_only_the_fields_the_resource_exposes() {
    let response = app()
        .oneshot(request("GET", "/restrictedCustomers", globex()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let keys: Vec<&String> = body["data"][0].as_object().unwrap().keys().collect();

    assert_eq!(keys, vec!["id", "name"]);
}

#[tokio::test]
async fn a_read_by_key_returns_only_the_fields_the_resource_exposes() {
    // The single-record route has no `select` at all, so before this fix it
    // could not restrict its columns even in principle.
    let response = app()
        .oneshot(request("GET", "/restrictedCustomers/1", globex()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let keys: Vec<&String> = body.as_object().unwrap().keys().collect();

    assert_eq!(keys, vec!["id", "name"]);
}

#[tokio::test]
async fn a_returning_connector_cannot_disclose_a_hidden_column_on_a_write() {
    let response = app()
        .oneshot(json_request(
            "POST",
            "/restrictedCustomers",
            globex(),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = body_json(response).await;
    let keys: Vec<&String> = body["data"][0].as_object().unwrap().keys().collect();

    assert_eq!(keys, vec!["id", "name"]);
}

#[tokio::test]
async fn the_tenant_discriminator_never_appears_on_any_route() {
    // The leak that matters most: the column name is the isolation model and
    // the value is the tenant's internal key. Asserted on the raw body so a
    // future response envelope cannot hide it somewhere else in the tree.
    for (method, uri, body) in [
        ("GET", "/restrictedCustomers", None),
        ("GET", "/restrictedCustomers/1", None),
        ("POST", "/restrictedCustomers", Some(json!({"name": "Alice"}))),
        ("PATCH", "/restrictedCustomers/1", Some(json!({"name": "Alice"}))),
    ] {
        let request = match &body {
            Some(payload) => json_request(method, uri, globex(), payload),
            None => request(method, uri, globex()),
        };

        let response = app().oneshot(request).await.unwrap();
        let status = response.status();
        let text = body_json(response).await.to_string();

        assert!(status.is_success(), "{method} {uri} answered {status}");
        assert!(!text.contains("tenant_key"), "{method} {uri} named the column");
        assert!(!text.contains("tenant-482"), "{method} {uri} leaked the key");
        assert!(!text.contains("salary"), "{method} {uri} leaked a hidden column");
    }
}

#[tokio::test]
async fn a_resource_that_declares_no_allowlist_still_returns_its_ordinary_columns() {
    // Empty `queryable_fields` still means "this resource has not opted into
    // hiding anything". Narrowing it to "expose nothing" would empty every
    // catalogue entry that has not enumerated its columns — see the rustdoc on
    // `ResourceDefinition::permits_field`.
    //
    // `tenant_key` is absent all the same, and that is the point of the next
    // test: it is not the catalogue's decision to make.
    let response = app()
        .oneshot(request("GET", "/customers", globex()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let keys: Vec<&String> = body["data"][0].as_object().unwrap().keys().collect();

    assert_eq!(keys, vec!["id", "name", "salary"]);
}

#[tokio::test]
async fn the_discriminator_is_hidden_even_from_a_resource_that_enumerates_nothing() {
    // The catalogue allowlist alone left this open, and it is the common case:
    // most entries do not enumerate their columns, so on a shared DataSource
    // every one of them returned the isolation column and the tenant's internal
    // key. §26 is not a policy an operator opts into, so the platform applies
    // it from the resolved isolation model regardless of the catalogue.
    for uri in ["/customers", "/readOnlyReport"] {
        let response = app().oneshot(request("GET", uri, globex())).await.unwrap();
        let text = body_json(response).await.to_string();

        assert!(!text.contains("tenant_key"), "{uri} named the column");
        assert!(!text.contains("tenant-482"), "{uri} leaked the key");
    }
}

#[tokio::test]
async fn a_tenant_on_a_dedicated_database_is_unaffected_by_the_discriminator_rule() {
    // There is no discriminator to hide when isolation is structural, so the
    // rule must not start removing a column that happens to share the name.
    let response = app()
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let keys: Vec<&String> = body["data"][0].as_object().unwrap().keys().collect();

    assert_eq!(keys, vec!["id", "name", "salary", "tenant_key"]);
}

#[tokio::test]
async fn the_allowlist_reaches_the_connector_as_a_projection_too() {
    // Defence in depth rather than the control itself: a connector honouring
    // `fields` never reads the hidden columns out of the backend at all. The
    // recording connector is used here because the assertion is about what was
    // *sent*, not what came back.
    let connector = support::RecordingConnector::new(vec![]);
    let app = support::app_with(runtime(), Arc::clone(&connector), open_permissions());

    app.oneshot(request("GET", "/restrictedCustomers", globex()))
        .await
        .unwrap();

    let asked_for: Vec<String> = connector
        .last_query()
        .1
        .fields
        .iter()
        .map(ToString::to_string)
        .collect();

    assert_eq!(asked_for, vec!["id".to_owned(), "name".to_owned()]);
}

#[tokio::test]
async fn an_unrestricted_resource_still_asks_for_the_connector_default_projection() {
    let connector = support::RecordingConnector::new(vec![]);
    let app = support::app_with(runtime(), Arc::clone(&connector), open_permissions());

    app.oneshot(request("GET", "/customers", globex())).await.unwrap();

    assert!(connector.last_query().1.fields.is_empty());
}
