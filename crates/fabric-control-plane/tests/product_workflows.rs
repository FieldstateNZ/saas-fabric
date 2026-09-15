//! Product workflows exercise authentication, immutable releases and real client documents.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support;
use std::sync::Arc;

use axum::{body::Body, Router};
use fabric_client_model::catalogue::{Catalogue, ClientProduct, ClientProductRequest, ProductActivity};
use fabric_client_model::{ClientId, Host};
use fabric_control_plane::{ChangeContext, ClientRepository};
use fabric_reconciliation::testing::FakeIdentityProvider;
use http::{header, StatusCode};
use serde_json::{json, Value};
use support::{as_operator, control_plane, control_plane_with_identity_provider, entity_tag, send, OPERATOR};

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

#[tokio::test]
async fn put_product_replaces_configuration_and_records_activity() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/product")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .body(Body::from(
                json!({
                    "displayName": "Acme",
                    "hosts": ["www.example.com"],
                    "legalName": "Acme Ltd",
                    "region": "NZ",
                    "timezone": "Pacific/Auckland",
                    "configuration": {},
                    "applications": []
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let new_revision = entity_tag(&response);
    assert_ne!(new_revision, plane.revision.to_string());

    let body = support::json(response).await;
    assert_eq!(body["client"]["revision"].as_str().unwrap(), new_revision);
    assert_eq!(
        body["product"]["activity"].as_array().unwrap().last().unwrap()["action"],
        "Client configuration updated"
    );
}

#[tokio::test]
async fn put_product_refuses_to_remove_an_assigned_application_and_leaves_the_client_unchanged() {
    // docs/delivery.md: this rule was disabled in `service/product.rs`,
    // confirmed to fail this test, and restored — see the report for the
    // agent's account of that step.
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
    command(
        &app.router,
        &saved["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
    )
    .await;

    let created = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = support::json(created).await;
    let revision = created["client"]["revision"].as_str().unwrap().to_owned();

    let mut without_application = client();
    without_application["applications"] = json!([]);

    let response = send(
        &app.router,
        as_operator("PUT", "/api/clients/newco/product")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{revision}\""))
            .body(Body::from(without_application.to_string()))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let after = send(
        &app.router,
        as_operator("GET", "/api/clients/newco/product")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let after = support::json(after).await;

    assert_eq!(after["product"]["applications"][0]["applicationId"], "analytics");
    assert_eq!(after["client"]["revision"].as_str().unwrap(), revision);
}

#[tokio::test]
async fn posting_the_catalogue_with_if_none_match_star_when_one_exists_is_conflict() {
    let app = control_plane();
    command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;

    let response = send(
        &app.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .header("if-none-match", "*")
            .body(Body::from(
                json!({"action":"createApplication","id":"other","name":"Other"}).to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn activity_merges_catalogue_and_client_events_newest_first() {
    // Both entries are written straight through the repository, at explicit
    // `at` values, rather than through the HTTP command flow — the test
    // fixtures share one fixed clock (`support::FixedClock`), so two events
    // produced through the API would tie on `at` and could not tell a real
    // sort from no sort at all.
    let plane = control_plane();

    let mut catalogue = Catalogue::default();
    catalogue.activity.push(ProductActivity {
        at: 100,
        operator: OPERATOR.into(),
        action: "Application created".into(),
        resource: "analytics".into(),
    });
    plane
        .repository
        .save_catalogue(&catalogue, None, &change())
        .await
        .unwrap();

    let acme = ClientId::try_new("acme").unwrap();
    let stored = plane.repository.get(&acme).await.unwrap();
    let request = ClientProductRequest {
        display_name: "Acme".into(),
        hosts: vec![Host::try_new("www.example.com").unwrap()],
        legal_name: "Acme Ltd".into(),
        region: "NZ".into(),
        timezone: "Pacific/Auckland".into(),
        configuration: std::collections::BTreeMap::default(),
        applications: vec![],
    };
    let product = ClientProduct {
        activity: vec![ProductActivity {
            at: 200,
            operator: OPERATOR.into(),
            action: "Client configuration updated".into(),
            resource: "acme".into(),
        }],
        ..ClientProduct::default()
    };
    let updated = stored.document.with_product(&request, &product).unwrap();
    plane
        .repository
        .update(&acme, &updated, &stored.revision, &change())
        .await
        .unwrap();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/activity").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = support::json(response).await;
    let activity = body["activity"].as_array().unwrap();

    assert_eq!(activity[0]["resource"], "acme", "the newer event must sort first");
    assert_eq!(activity[0]["at"].as_u64().unwrap(), 200);
    assert_eq!(activity[1]["resource"], "analytics");
    assert_eq!(activity[1]["at"].as_u64().unwrap(), 100);
}

#[tokio::test]
async fn operator_endpoint_returns_the_authenticated_subject() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/operator").body(Body::empty()).unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(support::json(response).await["subject"], OPERATOR);
}

#[tokio::test]
async fn reconciliation_does_not_touch_the_catalogue_revision() {
    // The regression this guards: `sweep` used to write a "reconciliation
    // pass completed" event into the catalogue after every convergence,
    // which bumped its revision on every `POST /api/reconciliation` —
    // including the one `converge::in_background` fires after an unrelated
    // client write — and made an operator's next catalogue save fail with a
    // conflict they never caused.
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider(provider);

    let before = plane.repository.catalogue().await.unwrap();
    assert_eq!(before.revision, None, "nothing has written the catalogue yet");

    let response = send(
        &plane.router,
        as_operator("POST", "/api/reconciliation")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let after = plane.repository.catalogue().await.unwrap();
    assert_eq!(after.revision, None, "a sweep must not write to the catalogue");
}

/// Shared by the tests above that write to the repository directly rather
/// than through an HTTP command.
fn change() -> ChangeContext {
    ChangeContext {
        requested_by: OPERATOR.into(),
        summary: "test fixture".into(),
    }
}
