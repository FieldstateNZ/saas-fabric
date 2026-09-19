//! The runtime catalogue is derived from published releases, through the
//! real router: what an operator publishes is what the runtime would be
//! given, and a second application cannot take a name the first declared.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support;

use axum::{body::Body, Router};
use http::StatusCode;
use serde_json::{json, Value};
use support::{as_operator, control_plane, send};

async fn command(router: &Router, revision: &Value, value: &Value) -> Value {
    let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
    let builder = if let Some(revision) = revision.as_str() {
        builder.header("if-match", format!("\"{revision}\""))
    } else {
        builder.header("if-none-match", "*")
    };
    let response = send(router, builder.body(Body::from(value.to_string())).unwrap()).await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

async fn refused(router: &Router, revision: &Value, value: &Value) -> (StatusCode, Value) {
    let request = as_operator("POST", "/api/catalogue")
        .header("content-type", "application/json")
        .header("if-match", format!("\"{}\"", revision.as_str().unwrap()))
        .body(Body::from(value.to_string()))
        .unwrap();
    let response = send(router, request).await;
    let status = response.status();
    (status, support::json(response).await)
}

async fn runtime(router: &Router) -> Value {
    let response = send(
        router,
        as_operator("GET", "/api/catalogue/runtime")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    support::json(response).await
}

/// A publishable definition with the given resources.
fn definition(name: &str, resources: &Value) -> Value {
    json!({"name":name, "description":"", "domain":"{client}.example.com",
        "components":[], "features":[],
        "plans":[{"id":"standard","name":"Standard","description":"","features":[],"configuration":{}}],
        "fields":[], "navigation":[], "resources":resources})
}

fn customers(queryable: &[&str]) -> Value {
    json!({"name":"customers","dataSource":"primary","collection":"customers","keyField":"id",
        "operations":["read","list","create"],"queryableFields":queryable})
}

#[tokio::test]
async fn the_runtime_catalogue_follows_the_newest_release_and_refuses_a_second_owner() {
    let app = control_plane();

    // Nothing published: an empty catalogue, honestly.
    let empty = runtime(&app.router).await;
    assert!(empty["resources"].as_array().unwrap().is_empty());

    let created = command(
        &app.router,
        &Value::Null,
        &json!({"action":"createApplication","id":"workspec","name":"Workspec"}),
    )
    .await;
    let resources =
        json!([customers(&["id", "name"]), {"name":"orders","dataSource":"primary","collection":"orders"}]);
    let saved = command(
        &app.router,
        &created["revision"],
        &json!({"action":"saveApplication","id":"workspec","definition":definition("Workspec", &resources)}),
    )
    .await;
    let published = command(
        &app.router,
        &saved["revision"],
        &json!({"action":"publishApplication","id":"workspec","note":"v1"}),
    )
    .await;

    let derived = runtime(&app.router).await;
    assert_eq!(derived["revision"], published["revision"]);
    let rows = derived["resources"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "customers");
    assert_eq!(rows[0]["application"], "workspec");
    assert_eq!(rows[0]["version"], 1);
    assert_eq!(rows[0]["dataSource"], "primary");
    assert_eq!(rows[0]["collection"], "customers");
    assert_eq!(rows[0]["keyField"], "id");
    assert_eq!(rows[0]["operations"], json!(["read", "list", "create"]));
    assert_eq!(rows[0]["queryableFields"], json!(["id", "name"]));
    // The wire's defaults apply to a resource that stated only the essentials.
    assert_eq!(rows[1]["name"], "orders");
    assert_eq!(rows[1]["keyField"], "id");
    assert_eq!(rows[1]["operations"], json!(["read", "list"]));
    assert_eq!(rows[1]["queryableFields"], json!([]));

    // A second application cannot take `customers`.
    let created = command(
        &app.router,
        &published["revision"],
        &json!({"action":"createApplication","id":"other","name":"Other"}),
    )
    .await;
    let saved = command(
        &app.router,
        &created["revision"],
        &json!({"action":"saveApplication","id":"other","definition":definition("Other", &json!([customers(&[])]))}),
    )
    .await;
    let (status, body) = refused(
        &app.router,
        &saved["revision"],
        &json!({"action":"publishApplication","id":"other","note":"v1"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("customers"), "{message}");
    assert!(message.contains("workspec"), "{message}");
    assert!(message.contains("other"), "{message}");

    // A newer release of the owner supersedes its own older definition.
    let resources = json!([customers(&["id", "name", "email"])]);
    let saved = command(
        &app.router,
        &saved["revision"],
        &json!({"action":"saveApplication","id":"workspec","definition":definition("Workspec", &resources)}),
    )
    .await;
    command(
        &app.router,
        &saved["revision"],
        &json!({"action":"publishApplication","id":"workspec","note":"v2"}),
    )
    .await;

    let derived = runtime(&app.router).await;
    let rows = derived["resources"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "orders was dropped by v2, and is no longer derived"
    );
    assert_eq!(rows[0]["version"], 2);
    assert_eq!(rows[0]["queryableFields"], json!(["id", "name", "email"]));
}

#[tokio::test]
async fn the_runtime_catalogue_needs_an_operator() {
    let app = control_plane();
    let response = send(
        &app.router,
        http::Request::builder()
            .method("GET")
            .uri("/api/catalogue/runtime")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
