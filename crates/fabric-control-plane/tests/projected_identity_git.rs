//! D01-19b: a product save never silently overwrites a stored projected
//! client that no longer matches the stored document's own projection.
//!
//! Each divergence is made the way a Git hand edit arrives: the stored
//! document is rendered to YAML, the text is edited, and the edited text is
//! parsed by `ClientDocument::parse` — the parser every repository read,
//! Git's included, goes through — then stored. The save that follows must
//! refuse and write nothing. A consistent stored projection must still let a
//! genuine host change or release upgrade through.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::body::Body;
use axum::Router;
use fabric_client_model::ClientId;
use fabric_control_plane::{AuditStream, AuditTrail as _, ClientRepository as _};
use http::{header, Response, StatusCode};
use serde_json::{json, Value};
use support::projected::{
    analytics_definition, catalogue_commands, hand_edit_in_git, newco_with_a_projected_client,
};
use support::{as_operator, control_plane, entity_tag, send, TestControlPlane};

const CALLBACK: &str = "https://newco.example.com/analytics/callback";

fn newco() -> ClientId {
    ClientId::try_new("newco").unwrap()
}

async fn save_product(router: &Router, revision: &str, hosts: &[&str], version: u64) -> Response<Body> {
    send(
        router,
        as_operator("PUT", "/api/clients/newco/product")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"displayName":"Newco","legalName":"Newco Ltd","region":"NZ","timezone":"Pacific/Auckland",
                    "hosts": hosts, "configuration":{},
                    "applications":[{"applicationId":"analytics","version":version,"planId":"standard","configuration":{}}]})
                .to_string(),
            ))
            .unwrap(),
    )
    .await
}

/// Asserts the save was refused naming `field`, and that nothing was
/// written: the revision did not move, the hand-edited text survives, and no
/// product update was audited.
async fn assert_refused_unchanged(plane: &TestControlPlane, revision: &str, field: &str) {
    let before = plane.repository.get(&newco()).await.unwrap();

    let response = save_product(&plane.router, revision, &["newco.example.com"], 1).await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = support::json(response).await;
    assert_eq!(body["error"]["code"], "projected_client_diverged", "{body}");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("analytics") && message.contains(field),
        "{message}"
    );
    assert!(!message.contains("https://"), "no callback is echoed: {message}");

    let after = plane.repository.get(&newco()).await.unwrap();
    assert_eq!(after.revision, before.revision, "nothing was written");
    assert_eq!(
        after.document.render().unwrap(),
        before.document.render().unwrap()
    );

    let stream = AuditStream::Client(newco());
    let audited = plane.audit.read(&stream).await.unwrap();
    assert!(audited
        .entries
        .iter()
        .all(|entry| entry.record().event() != "control_plane.audit.product_updated"));
}

#[tokio::test]
async fn a_callback_changed_in_git_refuses_the_next_product_save() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;
    let revision = hand_edit_in_git(&plane, |text| {
        text.replace(CALLBACK, "https://evil.example.com/analytics/callback")
    })
    .await;

    assert_refused_unchanged(&plane, &revision, "redirectUris").await;
}

#[tokio::test]
async fn a_callback_added_in_git_refuses_the_next_product_save() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;
    let revision = hand_edit_in_git(&plane, |text| {
        text.replace(
            &format!("- {CALLBACK}"),
            &format!("- {CALLBACK}\n        - https://extra.example.com/analytics/callback"),
        )
    })
    .await;
    let stored = plane.repository.get(&newco()).await.unwrap();
    assert_eq!(
        stored.document.client().identity.clients[0].redirect.uris().len(),
        3,
        "the edit must really have added a callback"
    );

    assert_refused_unchanged(&plane, &revision, "redirectUris").await;
}

#[tokio::test]
async fn a_projected_entry_removed_in_git_refuses_the_next_product_save() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;
    let stored = plane.repository.get(&newco()).await.unwrap();
    let mut identity = stored.document.client().identity.clone();
    identity.clients.clear();
    let without = stored.document.with_identity(identity).unwrap().render().unwrap();
    let revision = hand_edit_in_git(&plane, |_| without).await;

    assert_refused_unchanged(&plane, &revision, "missing").await;
}

#[tokio::test]
async fn a_restored_projection_lets_the_save_through() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;
    let edited = hand_edit_in_git(&plane, |text| {
        text.replace(CALLBACK, "https://evil.example.com/analytics/callback")
    })
    .await;
    let stored = plane.repository.get(&newco()).await.unwrap();
    let mut identity = serde_json::to_value(&stored.document.client().identity).unwrap();
    identity["clients"][0]["redirect"]["uris"] = json!(["https://newco.example.com/callback", CALLBACK]);

    let restored = send(
        &plane.router,
        as_operator("PUT", "/api/clients/newco/identity")
            .header(header::IF_MATCH, format!("\"{edited}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(identity.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(
        restored.status(),
        StatusCode::OK,
        "an identity edit back to the projection is allowed"
    );
    let revision = entity_tag(&restored);

    let response = save_product(&plane.router, &revision, &["newco.example.com"], 1).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_consistent_projection_still_takes_a_host_change_and_a_release_upgrade() {
    let plane = control_plane();
    let revision = newco_with_a_projected_client(&plane.router).await;

    let response = save_product(
        &plane.router,
        &revision,
        &["newco.example.com", "portal.example.com"],
        1,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let revision = entity_tag(&response);

    let catalogue = support::json(
        send(
            &plane.router,
            as_operator("GET", "/api/catalogue").body(Body::empty()).unwrap(),
        )
        .await,
    )
    .await;
    let mut next = analytics_definition();
    next["domain"] = json!("{client}-reports.example.com");
    catalogue_commands(
        &plane.router,
        catalogue["revision"].clone(),
        vec![
            json!({"action":"saveApplication","id":"analytics","definition":next}),
            json!({"action":"publishApplication","id":"analytics","note":"Second release"}),
        ],
    )
    .await;

    let response = save_product(
        &plane.router,
        &revision,
        &["newco.example.com", "portal.example.com"],
        2,
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let stored = plane.repository.get(&newco()).await.unwrap();
    let uris: Vec<String> = stored.document.client().identity.clients[0]
        .redirect
        .uris()
        .iter()
        .map(|uri| uri.as_str().to_owned())
        .collect();
    assert!(
        uris.contains(&"https://newco-reports.example.com/callback".to_owned()),
        "{uris:?}"
    );
    assert!(
        uris.contains(&"https://portal.example.com/analytics/callback".to_owned()),
        "{uris:?}"
    );
}

#[tokio::test]
async fn a_hand_declared_client_round_trips_through_a_product_save() {
    let plane = control_plane();
    let revision = newco_with_a_projected_client(&plane.router).await;
    let stored = plane.repository.get(&newco()).await.unwrap();
    let mut identity = serde_json::to_value(&stored.document.client().identity).unwrap();
    let mut manual = identity["clients"][0].clone();
    manual["id"] = json!("backoffice");
    manual["redirect"]["uris"] = json!(["https://backoffice.example.com/callback"]);
    identity["clients"].as_array_mut().unwrap().push(manual.clone());
    let declared = send(
        &plane.router,
        as_operator("PUT", "/api/clients/newco/identity")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(identity.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(declared.status(), StatusCode::OK);
    let revision = entity_tag(&declared);

    let response = save_product(&plane.router, &revision, &["newco.example.com"], 1).await;
    assert_eq!(response.status(), StatusCode::OK);

    let stored = plane.repository.get(&newco()).await.unwrap();
    let kept: Value = serde_json::to_value(
        stored
            .document
            .client()
            .identity
            .clients
            .iter()
            .find(|client| client.id.as_str() == "backoffice")
            .expect("the hand-declared client survives"),
    )
    .unwrap();
    assert_eq!(kept, manual);
}
