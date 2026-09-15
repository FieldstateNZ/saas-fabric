//! Product workflows exercise authentication, immutable releases and real client documents.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support;
use axum::{body::Body, Router};
use http::StatusCode;
use serde_json::{json, Value};
use support::{as_operator, control_plane, send};

async fn command(router: &Router, revision: &Value, value: Value) -> Value {
    let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
    let builder = if let Some(revision) = revision.as_str() {
        builder.header("if-match", format!("\"{revision}\""))
    } else {
        builder.header("if-none-match", "*")
    };
    let response = send(router, builder.body(Body::from(value.to_string())).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    support::json(response).await
}
fn definition() -> Value {
    json!({"name":"Analytics", "description":"Reports", "domain":"{client}.example.com",
        "components":[{"id":"reports", "name":"Reports", "kind":"container", "reference":"registry.example.com/reports", "version":"1.0.0", "required":false,"policy":"manual"}],
        "features":[{"id":"reporting","name":"Reporting","description":"", "implementedBy":["reports"]}],
        "plans":[{"id":"standard","name":"Standard","description":"","features":["reporting"],"configuration":{"seats":"10"}}],
        "fields":[{"key":"team","label":"Team","kind":"text","required":true,"default":null,"options":[],"description":""}],
        "navigation":[{"label":"Reports","route":"/reports","feature":"reporting","permission":"reports.read"}]})
}
fn client() -> Value {
    json!({"displayName":"Acme", "legalName":"Acme Ltd", "region":"NZ", "timezone":"Pacific/Auckland", "hosts":["newco.example.com"], "configuration":{},
        "applications":[{"applicationId":"analytics","version":1,"planId":"standard","configuration":{"team":"Finance"}}]})
}
#[tokio::test]
async fn published_assignment_is_immutable_and_creates_identity() {
    let app = control_plane();
    let first = command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;
    let saved = command(
        &app.router,
        &first["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":definition()}),
    )
    .await;
    let published = command(
        &app.router,
        &saved["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
    )
    .await;
    let mut changed = definition();
    changed["name"] = json!("Next draft");
    let edited = command(
        &app.router,
        &published["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":changed}),
    )
    .await;
    assert_eq!(
        edited["catalogue"]["applications"][0]["releases"][0]["definition"]["name"],
        "Analytics"
    );
    let response = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let created = support::json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["resolved"][0]["components"][0]["id"], "reports");
    assert_eq!(created["resolved"][0]["navigation"][0]["route"], "/reports");
    assert_eq!(
        created["product"]["applications"][0]["release"]["definition"]["name"],
        "Analytics"
    );
    let identity = send(
        &app.router,
        as_operator("GET", "/api/clients/newco/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let identity = support::json(identity).await;
    assert!(identity.to_string().contains("analytics"));
    assert!(identity
        .to_string()
        .contains("https://newco.example.com/callback"));
    let duplicate = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    let stale = send(
        &app.router,
        as_operator("PUT", "/api/clients/newco/product")
            .header("content-type", "application/json")
            .header("if-match", "\"stale\"")
            .body(Body::from(client().to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
}
#[tokio::test]
async fn catalogue_requires_auth_and_conditional_writes() {
    let app = control_plane();
    for path in [
        "/api/catalogue",
        "/api/activity",
        "/api/operator",
        "/api/clients/unknown/product",
    ] {
        let response = send(
            &app.router,
            http::Request::builder().uri(path).body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let value = json!({"action":"createApplication","id":"analytics","name":"Analytics"});
    command(&app.router, &Value::Null, value.clone()).await;
    let response = send(
        &app.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .header("if-none-match", "*")
            .body(Body::from(value.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
#[tokio::test]
async fn invalid_publication_and_unpublished_assignment_are_rejected() {
    let app = control_plane();
    let draft = command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;
    let response = send(
        &app.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{}\"", draft["revision"].as_str().unwrap()))
            .body(Body::from(
                json!({"action":"publishApplication","id":"analytics","note":"Invalid"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
