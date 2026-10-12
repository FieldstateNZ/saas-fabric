//! The durable audit trail through the real router (ADR 0028): every audited
//! operation lands in its stream's hash chain, the lifecycle events (S16,
//! S17) among them, and the activity view stays bounded while the trail
//! stays complete.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod support;

use axum::body::Body;
use axum::Router;
use fabric_client_model::ClientId;
use fabric_control_plane::{AuditStream, AuditTrail as _};
use http::StatusCode;
use serde_json::{json, Value};
use support::{as_operator, control_plane, entity_tag, send, OPERATOR};

/// The activity view's bound, as `GET /api/activity` reports it.
const VIEW_LIMIT: u64 = 200;

async fn post(router: &Router, path: &str, revision: &str, body: &Value) -> (StatusCode, String) {
    let request = as_operator("POST", path)
        .header("content-type", "application/json")
        .header("if-match", format!("\"{revision}\""))
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = send(router, request).await;
    let status = response.status();
    let tag = if status == StatusCode::OK {
        entity_tag(&response)
    } else {
        String::new()
    };
    (status, tag)
}

/// Runs one catalogue command and returns the catalogue's new revision.
async fn command(router: &Router, revision: Option<&str>, body: &Value) -> String {
    let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
    let builder = match revision {
        Some(revision) => builder.header("if-match", format!("\"{revision}\"")),
        None => builder.header("if-none-match", "*"),
    };
    let response = send(router, builder.body(Body::from(body.to_string())).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    support::json(response).await["revision"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn the_lifecycle_events_are_chained_into_the_clients_stream() {
    let plane = control_plane();
    let acme = AuditStream::Client(ClientId::try_new("acme").unwrap());

    let (status, tombstoned) = post(
        &plane.router,
        "/api/clients/acme/tombstone",
        plane.revision.as_str(),
        &json!({"reason": "Contract ended"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, confirmed) = post(
        &plane.router,
        "/api/clients/acme/deletion-confirmation",
        &tombstoned,
        &json!({"confirmClientId": "acme"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let stored = plane.audit.read(&acme).await.unwrap();
    let recorded: Vec<(&str, &str, Option<&str>)> = stored
        .entries
        .iter()
        .map(|entry| {
            let record = entry.record();
            (record.event(), record.requested_by(), record.field("revision"))
        })
        .collect();

    assert_eq!(
        recorded,
        [
            (
                "control_plane.audit.client_tombstoned",
                OPERATOR,
                Some(tombstoned.as_str())
            ),
            (
                "control_plane.audit.client_deletion_confirmed",
                OPERATOR,
                Some(confirmed.as_str())
            ),
        ]
    );
    assert_eq!(stored.verify().unwrap().map(|head| head.sequence), Some(2));
}

#[tokio::test]
async fn a_refused_lifecycle_write_appends_nothing() {
    let plane = control_plane();
    let acme = AuditStream::Client(ClientId::try_new("acme").unwrap());

    let (status, _) = post(
        &plane.router,
        "/api/clients/acme/deletion-confirmation",
        plane.revision.as_str(),
        &json!({"confirmClientId": "acme"}),
    )
    .await;

    assert_ne!(
        status,
        StatusCode::OK,
        "confirmation before a tombstone is refused"
    );
    assert!(plane.audit.read(&acme).await.unwrap().entries.is_empty());
}

#[tokio::test]
async fn the_activity_view_stays_bounded_while_the_audit_trail_stays_complete() {
    let plane = control_plane();
    let commands = VIEW_LIMIT + 5;

    let mut revision = None;
    for n in 0..commands {
        let body =
            json!({"action": "createApplication", "id": format!("app-{n}"), "name": format!("App {n}")});
        revision = Some(command(&plane.router, revision.as_deref(), &body).await);
    }

    let response = send(
        &plane.router,
        as_operator("GET", "/api/activity").body(Body::empty()).unwrap(),
    )
    .await;
    let view = support::json(response).await;
    let shown = view["activity"].as_array().unwrap().len() as u64;
    let omitted = view["omitted"].as_u64().unwrap();

    assert_eq!(shown, VIEW_LIMIT, "the view is bounded");
    assert_eq!(view["limit"], VIEW_LIMIT);
    assert!(
        omitted >= commands - VIEW_LIMIT,
        "the view says what it left out: {omitted}"
    );

    let catalogue = plane.audit.read(&AuditStream::Catalogue).await.unwrap();
    assert_eq!(
        catalogue.entries.len() as u64,
        commands,
        "the trail keeps every command"
    );
    assert_eq!(
        catalogue.verify().unwrap().map(|head| head.sequence),
        Some(commands)
    );
    assert_eq!(
        catalogue.entries[0].record().field("entry"),
        Some("app-0"),
        "the oldest command, which the view no longer shows, is still in the trail"
    );
}
