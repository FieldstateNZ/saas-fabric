//! D01-19b through the real router: a projected identity client is the
//! product's. An unsupported hand edit in the identity provider is reported
//! as `handEdited` and left in place; only an explicit, audited re-assertion
//! overwrites it. An identity edit to a projected client is refused, except
//! on a retired client, where it is the only remediation left.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::Router;
use fabric_client_model::{ClientId, OidcClientId, RealmName};
use fabric_control_plane::{AuditStream, AuditTrail as _};
use fabric_reconciliation::testing::FakeIdentityProvider;
use http::{header, Response, StatusCode};
use serde_json::{json, Value};
use support::projected::newco_with_a_projected_client;
use support::{as_operator, control_plane, control_plane_with_identity_provider, entity_tag, send, OPERATOR};

const REASSERTED: &str = "control_plane.audit.identity_projection_reasserted";

async fn get_identity(router: &Router, client: &str) -> (String, Value) {
    let response = send(
        router,
        as_operator("GET", &format!("/api/clients/{client}/identity"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let tag = entity_tag(&response);
    (tag, support::json(response).await)
}

async fn sweep(router: &Router) {
    let response = send(
        router,
        as_operator("POST", "/api/reconciliation")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn reassert(router: &Router, client: &str, revision: &str) -> Response<Body> {
    send(
        router,
        as_operator("POST", &format!("/api/clients/{client}/identity/reassertion"))
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

async fn put_identity(router: &Router, client: &str, revision: &str, identity: &Value) -> Response<Body> {
    send(
        router,
        as_operator("PUT", &format!("/api/clients/{client}/identity"))
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(identity.to_string()))
            .unwrap(),
    )
    .await
}

/// `acme` converged, then its `web` client switched off in the provider's
/// own console.
async fn acme_disabled_by_hand() -> (support::TestControlPlane, Arc<FakeIdentityProvider>) {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider(provider.clone());
    sweep(&plane.router).await;

    let realm = RealmName::try_new("acme").unwrap();
    let mut state = provider.realm(&realm).unwrap();
    state
        .clients
        .get_mut(&OidcClientId::try_new("web").unwrap())
        .unwrap()
        .enabled = false;
    provider.seed_realm(realm, state);
    provider.clear_calls();

    (plane, provider)
}

fn web_enabled(provider: &FakeIdentityProvider) -> bool {
    provider
        .realm(&RealmName::try_new("acme").unwrap())
        .unwrap()
        .clients[&OidcClientId::try_new("web").unwrap()]
        .enabled
}

#[tokio::test]
async fn a_hand_edit_is_reported_and_left_in_place_however_often_anyone_sweeps() {
    let (plane, provider) = acme_disabled_by_hand().await;

    for _ in 0..3 {
        sweep(&plane.router).await;
    }

    let (_, identity) = get_identity(&plane.router, "acme").await;
    assert_eq!(identity["reconciliation"]["status"], "handEdited", "{identity}");
    assert_eq!(
        identity["reconciliation"]["handEdits"],
        json!([{"client": "web", "field": "clientDisabled"}])
    );
    assert_eq!(
        provider.calls(),
        ["observe_realm:acme"; 3],
        "a hand edit is never written over"
    );
    assert!(!web_enabled(&provider));

    let client = send(
        &plane.router,
        as_operator("GET", "/api/clients/acme")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let client = support::json(client).await;
    assert!(
        client["lifecycle"]["unmet"]
            .as_array()
            .unwrap()
            .iter()
            .any(|unmet| unmet["condition"] == "identity"),
        "a hand-edited client is not ready: {client}"
    );
}

#[tokio::test]
async fn reasserting_overwrites_the_hand_edit_and_records_it_in_the_audit_trail() {
    let (plane, provider) = acme_disabled_by_hand().await;
    sweep(&plane.router).await;
    let (revision, _) = get_identity(&plane.router, "acme").await;

    let response = reassert(&plane.router, "acme", &revision).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        entity_tag(&response),
        revision,
        "a re-assertion writes no desired state"
    );
    let body = support::json(response).await;
    assert_eq!(body["reconciliation"]["status"], "drifted", "{body}");
    assert_eq!(body["reconciliation"]["handEdits"], json!([]));
    assert!(web_enabled(&provider));

    let stream = AuditStream::Client(ClientId::try_new("acme").unwrap());
    let stored = plane.audit.read(&stream).await.unwrap();
    let record = stored
        .entries
        .iter()
        .map(fabric_control_plane::AuditEntry::record)
        .find(|record| record.event() == REASSERTED)
        .expect("the re-assertion must be audited");
    assert_eq!(record.requested_by(), OPERATOR);
    assert_eq!(record.field("revision"), Some(revision.as_str()));
    assert_eq!(record.field("overwritten"), Some("web:clientDisabled"));
    assert_eq!(record.field("status"), Some("drifted"));

    provider.clear_calls();
    sweep(&plane.router).await;
    assert_eq!(
        provider.calls(),
        ["observe_realm:acme"],
        "re-asserted, then converged: no loop"
    );
}

/// A re-assertion that fails part-way is audited with what it did: the
/// overwrite the provider confirmed, the one whose call failed, and the count.
#[tokio::test]
async fn a_partly_failed_reassertion_audits_the_overwrite_that_took_effect() {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider(provider.clone());
    let (revision, identity) = get_identity(&plane.router, "acme").await;
    let mut two =
        json!({"realm": identity["realm"], "roles": identity["roles"], "clients": identity["clients"]});
    let mut admin = identity["clients"][0].clone();
    admin["id"] = json!("admin");
    two["clients"].as_array_mut().unwrap().push(admin);
    let written = put_identity(&plane.router, "acme", &revision, &two).await;
    assert_eq!(written.status(), StatusCode::OK);
    tokio::task::yield_now().await;
    sweep(&plane.router).await;

    let realm = RealmName::try_new("acme").unwrap();
    let mut state = provider.realm(&realm).unwrap();
    for client in state.clients.values_mut() {
        client.enabled = false;
    }
    provider.seed_realm(realm, state);
    provider.fail_call(
        "update_oidc_client:acme:admin",
        fabric_reconciliation::ProviderError::Unavailable {
            detail: "refused mid-plan".to_owned(),
        },
    );
    let (revision, _) = get_identity(&plane.router, "acme").await;

    let response = reassert(&plane.router, "acme", &revision).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        support::json(response).await["reconciliation"]["status"],
        "failed"
    );
    assert!(web_enabled(&provider), "the confirmed overwrite took effect");

    let stream = AuditStream::Client(ClientId::try_new("acme").unwrap());
    let stored = plane.audit.read(&stream).await.unwrap();
    let record = stored
        .entries
        .iter()
        .map(fabric_control_plane::AuditEntry::record)
        .find(|record| record.event() == REASSERTED)
        .expect("a failed re-assertion is still audited");
    assert_eq!(record.field("status"), Some("failed"));
    assert_eq!(record.field("actions"), Some("1"));
    assert_eq!(record.field("overwritten"), Some("web:clientDisabled"));
    assert_eq!(record.field("unconfirmed"), Some("admin:clientDisabled"));
}

#[tokio::test]
async fn a_reassertion_names_the_revision_it_reviewed() {
    let (plane, provider) = acme_disabled_by_hand().await;

    let stale = reassert(&plane.router, "acme", "stale").await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(support::json(stale).await["error"]["code"], "revision_conflict");

    let unconditional = send(
        &plane.router,
        as_operator("POST", "/api/clients/acme/identity/reassertion")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unconditional.status(), StatusCode::PRECONDITION_REQUIRED);

    assert!(!web_enabled(&provider), "a refused re-assertion writes nothing");
    let stream = AuditStream::Client(ClientId::try_new("acme").unwrap());
    let stored = plane.audit.read(&stream).await.unwrap();
    assert!(stored
        .entries
        .iter()
        .all(|entry| entry.record().event() != REASSERTED));
}

#[tokio::test]
async fn a_reassertion_needs_an_operator_and_a_converging_deployment() {
    let plane = control_plane();

    let anonymous = send(
        &plane.router,
        http::Request::builder()
            .method("POST")
            .uri("/api/clients/acme/identity/reassertion")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let unconverged = reassert(&plane.router, "acme", plane.revision.as_str()).await;
    assert_eq!(unconverged.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        support::json(unconverged).await["error"]["code"],
        "convergence_unavailable"
    );
}

#[tokio::test]
async fn a_retired_client_can_still_be_reasserted() {
    let (plane, provider) = acme_disabled_by_hand().await;
    let (revision, _) = get_identity(&plane.router, "acme").await;
    let tombstoned = send(
        &plane.router,
        as_operator("POST", "/api/clients/acme/tombstone")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"reason": "Contract ended"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(tombstoned.status(), StatusCode::OK);
    let retired = entity_tag(&tombstoned);

    let response = reassert(&plane.router, "acme", &retired).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(web_enabled(&provider));
}

/// `newco`'s identity, with the projected `analytics` client's callbacks
/// replaced by `uris` — or the client removed, for `None`.
fn newco_identity(identity: &Value, uris: Option<&[&str]>) -> Value {
    let mut edited = json!({
        "realm": identity["realm"],
        "roles": identity["roles"],
        "clients": identity["clients"],
    });
    match uris {
        Some(uris) => edited["clients"][0]["redirect"]["uris"] = json!(uris),
        None => edited["clients"] = json!([]),
    }
    edited
}

#[tokio::test]
async fn an_identity_edit_to_a_projected_client_is_refused() {
    let plane = control_plane();
    let revision = newco_with_a_projected_client(&plane.router).await;
    let (_, identity) = get_identity(&plane.router, "newco").await;
    assert_eq!(identity["clients"][0]["id"], "analytics");

    for edited in [
        newco_identity(&identity, Some(&["https://newco.example.com/callback"])),
        newco_identity(&identity, None),
    ] {
        let response = put_identity(&plane.router, "newco", &revision, &edited).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = support::json(response).await;
        assert_eq!(body["error"]["code"], "projected_client_product_owned", "{body}");
    }

    let mut roles_only = newco_identity(&identity, None);
    roles_only["clients"] = identity["clients"].clone();
    let mut roles = identity["roles"].as_array().unwrap().clone();
    roles.push(json!("Analyst"));
    roles_only["roles"] = json!(roles);
    let response = put_identity(&plane.router, "newco", &revision, &roles_only).await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an edit that leaves the projection alone is not refused: {body}"
    );
}

#[tokio::test]
async fn a_retired_clients_projected_client_stays_open_to_remediation() {
    let plane = control_plane();
    let revision = newco_with_a_projected_client(&plane.router).await;
    let (_, identity) = get_identity(&plane.router, "newco").await;
    let tombstoned = send(
        &plane.router,
        as_operator("POST", "/api/clients/newco/tombstone")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"reason": "Contract ended"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(tombstoned.status(), StatusCode::OK);
    let retired = entity_tag(&tombstoned);

    let narrowed = newco_identity(&identity, Some(&["https://newco.example.com/callback"]));
    let response = put_identity(&plane.router, "newco", &retired, &narrowed).await;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "removing a compromised callback from a retired client must stay possible"
    );
}
