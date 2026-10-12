//! Restoring a tombstoned client through the real router (D01-14; ADR 0021,
//! "Restoring a tombstone"; #108 comments 6113749198 item 3 and 6119161317
//! item 2): an explicit operator write at the tombstoned revision, held
//! wherever no protected continuity store exists, refused for a confirmed
//! deletion or unretained placements, reserved in the continuity store
//! before the document is written, audited through the durable trail, and
//! leaving the client saved intent until its identity is applied again at
//! the new revision with a consistent projection.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::indexing_slicing
)]

mod support;

use std::sync::Arc;

use axum::body::Body;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};
use fabric_control_plane::testing::ContinuityEvent;
use fabric_control_plane::{AuditStream, AuditTrail, ClientRepository, ContinuityRefusal};
use fabric_platform_management::{DesiredRevision, PlacementState};
use fabric_reconciliation::ReconciliationOutcome;
use http::header::{ETAG, RETRY_AFTER};
use http::StatusCode;
use serde_json::{json, Value};
use support::lifecycle::{
    conditions, confirm_deletion, declared_data_source, get, placement, post,
    publish_one_application_with_a_resource, sink, tenants_document, tombstone, GLOBEX,
};
use support::newco::{converged_on, identity_unmet, provider_callbacks, settled, sweep};
use support::platform_fixture::{platform_binding, FakeRepository};
use support::projected::hand_edit_in_git;
use support::{
    as_operator, control_plane, control_plane_with_no_continuity, control_plane_with_publication,
    json as body_of, send, TempDir, TestControlPlane, OPERATOR,
};

fn id(value: &str) -> ClientId {
    ClientId::try_new(value).unwrap()
}

async fn restore(plane: &TestControlPlane, client: &str, revision: &str) -> (StatusCode, Value) {
    post(
        &plane.router,
        &format!("/api/clients/{client}/restoration"),
        Some(revision),
        &json!({"reason": "Contract renewed"}),
    )
    .await
}

async fn revision_of(plane: &TestControlPlane, client: &str) -> String {
    plane
        .repository
        .get(&id(client))
        .await
        .unwrap()
        .revision
        .as_str()
        .to_owned()
}

async fn audited(plane: &TestControlPlane, client: &str) -> Vec<String> {
    let stored = plane.audit.read(&AuditStream::Client(id(client))).await.unwrap();
    stored
        .entries
        .iter()
        .map(|entry| entry.record().event().to_owned())
        .collect()
}

fn continuity_events(plane: &TestControlPlane) -> Vec<ContinuityEvent> {
    plane.continuity.as_ref().unwrap().events()
}

/// `acme` tombstoned in `plane`, and the revision the tombstone produced.
async fn tombstoned(plane: &TestControlPlane) -> String {
    let (status, body) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["revision"].as_str().unwrap().to_owned()
}

/// `globex` placed, applied and published beside `acme`, then tombstoned:
/// the plane, the fake platform, the publication directory, the revision
/// before the tombstone and the tombstoned revision.
async fn a_retired_published_client() -> (TestControlPlane, Arc<FakeRepository>, TempDir, String, String) {
    let dir = TempDir::new("restore-published");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    let before = plane
        .repository
        .insert(&ClientDocument::parse(GLOBEX).unwrap())
        .unwrap();
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(
        DesiredRevision::new("pl-1"),
        vec![placement("acme"), placement("globex")],
    );
    plane.statuses.record(
        &id("globex"),
        before.clone(),
        &ReconciliationOutcome::converged(),
        1,
    );
    publish_one_application_with_a_resource(&plane.router).await;
    post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    let (_, ready) = get(&plane.router, "/api/clients/globex").await;
    assert_eq!(ready["lifecycle"]["state"], "ready", "{ready}");

    let (status, retired) = tombstone(&plane.router, "globex", before.as_str()).await;
    assert_eq!(status, StatusCode::OK, "{retired}");
    let retired = retired["revision"].as_str().unwrap().to_owned();
    (plane, fake, dir, before.as_str().to_owned(), retired)
}

// Held: no deployment has a continuity store.

#[tokio::test]
async fn a_restore_is_held_where_no_continuity_store_exists_and_writes_nothing() {
    let plane = control_plane_with_no_continuity(None, None);
    let retired = tombstoned(&plane).await;
    let audited_before = audited(&plane, "acme").await;

    let response = send(
        &plane.router,
        as_operator("POST", "/api/clients/acme/restoration")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{retired}\""))
            .body(Body::from(json!({"reason": "Contract renewed"}).to_string()))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let retry_after = response.headers().get(RETRY_AFTER).cloned();
    let body = body_of(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "lifecycle_continuity_unavailable");
    assert!(
        body["error"]["message"].as_str().unwrap().contains("held"),
        "{body}"
    );
    assert_eq!(retry_after, None, "waiting does not create a continuity store");
    assert_eq!(revision_of(&plane, "acme").await, retired, "nothing was written");
    let (_, after) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(after["lifecycle"]["state"], "tombstoned", "{after}");
    assert_eq!(after["lifecycle"]["restorations"], json!([]));
    assert_eq!(
        audited(&plane, "acme").await,
        audited_before,
        "nothing was audited"
    );
}

#[tokio::test]
async fn the_hold_comes_after_the_revision_and_lifecycle_checks() {
    let plane = control_plane_with_no_continuity(None, None);

    let (status, body) = restore(&plane, "acme", plane.revision.as_str()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "client_not_tombstoned");

    let retired = tombstoned(&plane).await;
    let (status, body) = restore(&plane, "acme", plane.revision.as_str()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "revision_conflict");

    let (_, confirmed) = confirm_deletion(&plane.router, "acme", &retired).await;
    let (status, body) = restore(&plane, "acme", confirmed["revision"].as_str().unwrap()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "deletion_confirmed_not_restorable");
}

// The write, with the test-only continuity store.

#[tokio::test]
async fn a_restore_reverses_the_tombstone_forward_and_leaves_the_client_saved_intent() {
    let (plane, fake, dir, before, retired) = a_retired_published_client().await;
    plane.continuity.as_ref().unwrap().enroll(&id("globex"));
    let published = tenants_document(&dir);
    let placements = fake.read_placements("lucentroot").await.unwrap();
    let stored_before = plane.repository.get(&id("globex")).await.unwrap();

    let response = send(
        &plane.router,
        as_operator("POST", "/api/clients/globex/restoration")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{retired}\""))
            .body(Body::from(json!({"reason": "  Contract renewed  "}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let etag = response.headers().get(ETAG).unwrap().to_str().unwrap().to_owned();
    let restored = body_of(response).await;

    // A forward write: a new revision, never the one before the tombstone.
    let revision = restored["revision"].as_str().unwrap().to_owned();
    assert_eq!(etag, format!("\"{revision}\""));
    assert_ne!(revision, retired);
    assert_ne!(revision, before);

    // Saved intent again: identity is pending at the new revision.
    let lifecycle = &restored["lifecycle"];
    assert_eq!(lifecycle["state"], "incomplete", "{restored}");
    assert_eq!(conditions(lifecycle), ["identity"], "{restored}");
    assert_eq!(
        lifecycle["unmet"][0]["detail"],
        "identity has not been applied at this revision yet"
    );
    assert_eq!(lifecycle["tombstone"], Value::Null);
    assert_eq!(lifecycle["withdrawal"], Value::Null);

    // The tombstone is kept, with who reversed it and why.
    let history = &lifecycle["restorations"];
    assert_eq!(history.as_array().unwrap().len(), 1, "{restored}");
    assert_eq!(history[0]["tombstone"]["reason"], "Contract ended");
    assert_eq!(history[0]["tombstone"]["by"], OPERATOR);
    assert_eq!(history[0]["reason"], "Contract renewed");
    assert_eq!(history[0]["by"], OPERATOR);

    // Reserved at the tombstoned revision, then completed at the new one.
    assert_eq!(
        continuity_events(&plane),
        [
            ContinuityEvent::Reserved(id("globex"), ClientRevision::try_new(&retired).unwrap()),
            ContinuityEvent::Completed(id("globex"), ClientRevision::try_new(&revision).unwrap()),
        ]
    );

    // Audited through the durable trail, naming the revision and the tombstone.
    let stored = plane
        .audit
        .read(&AuditStream::Client(id("globex")))
        .await
        .unwrap();
    let entry = stored.entries.last().unwrap().record();
    assert_eq!(entry.event(), "control_plane.audit.client_restored");
    assert_eq!(entry.requested_by(), OPERATOR);
    assert_eq!(entry.field("revision"), Some(revision.as_str()));
    assert!(entry.field("tombstonedAt").is_some());
    assert!(stored.verify().is_ok());

    // Nothing else moved: data intent, identity, secrets, placements and the
    // published tenants document are as the tombstone left them.
    let stored_after = plane.repository.get(&id("globex")).await.unwrap();
    let (was, is) = (stored_before.document.client(), stored_after.document.client());
    assert_eq!(was.data, is.data);
    assert_eq!(was.identity, is.identity);
    assert_eq!(was.secrets, is.secrets);
    assert_eq!(
        fake.read_placements("lucentroot").await.unwrap().placements,
        placements.placements
    );
    assert_eq!(fake.placement_writes(), 0);
    assert_eq!(tenants_document(&dir), published);
    let (status, pass) = post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{pass}");
    assert_eq!(
        tenants_document(&dir),
        published,
        "no binding or resource revision moved"
    );

    // Ready again only once identity is applied at the restored revision.
    plane.statuses.record(
        &id("globex"),
        ClientRevision::try_new(&revision).unwrap(),
        &ReconciliationOutcome::converged(),
        2,
    );
    let (_, ready) = get(&plane.router, "/api/clients/globex").await;
    assert_eq!(ready["lifecycle"]["state"], "ready", "{ready}");
}

#[tokio::test]
async fn a_restored_client_can_be_retired_again_and_its_history_only_grows() {
    let plane = control_plane();
    plane.continuity.as_ref().unwrap().enroll(&id("acme"));
    let first = tombstoned(&plane).await;
    let (_, restored) = restore(&plane, "acme", &first).await;

    let (status, again) = tombstone(&plane.router, "acme", restored["revision"].as_str().unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["lifecycle"]["state"], "tombstoned");
    assert_eq!(again["lifecycle"]["restorations"].as_array().unwrap().len(), 1);

    let (status, twice) = restore(&plane, "acme", again["revision"].as_str().unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{twice}");
    assert_eq!(twice["lifecycle"]["restorations"].as_array().unwrap().len(), 2);
    let (_, listed) = get(&plane.router, "/api/clients").await;
    assert_eq!(
        listed["clients"][0]["lifecycle"]["restorations"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "{listed}"
    );
}

// Refusals before any write.

#[tokio::test]
async fn a_deletion_confirmed_client_is_not_restored_and_nothing_is_reserved() {
    let plane = control_plane();
    plane.continuity.as_ref().unwrap().enroll(&id("acme"));
    let retired = tombstoned(&plane).await;
    let (_, confirmed) = confirm_deletion(&plane.router, "acme", &retired).await;
    let revision = confirmed["revision"].as_str().unwrap();

    let (status, body) = restore(&plane, "acme", revision).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "deletion_confirmed_not_restorable");
    assert_eq!(revision_of(&plane, "acme").await, revision);
    assert_eq!(continuity_events(&plane), []);
}

#[tokio::test]
async fn a_restore_needs_the_current_revision_an_operator_and_a_reason() {
    let plane = control_plane();
    plane.continuity.as_ref().unwrap().enroll(&id("acme"));
    let retired = tombstoned(&plane).await;

    let (status, body) = post(
        &plane.router,
        "/api/clients/acme/restoration",
        None,
        &json!({"reason": "Contract renewed"}),
    )
    .await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED, "{body}");

    let (status, body) = restore(&plane, "acme", plane.revision.as_str()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "revision_conflict");

    let (status, body) = post(
        &plane.router,
        "/api/clients/acme/restoration",
        Some(&retired),
        &json!({"reason": " "}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "invalid_restoration_reason");

    let (status, _) = post(
        &plane.router,
        "/api/clients/acme/restoration",
        Some(&retired),
        &json!({"reason": "x", "confirmClientId": "acme"}),
    )
    .await;
    assert!(status.is_client_error());

    let unauthenticated = send(
        &plane.router,
        http::Request::builder()
            .method("POST")
            .uri("/api/clients/acme/restoration")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{retired}\""))
            .body(Body::from(json!({"reason": "x"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let (status, _) = restore(&plane, "initech", &retired).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    assert_eq!(revision_of(&plane, "acme").await, retired, "nothing was written");
    assert_eq!(continuity_events(&plane), []);
}

#[tokio::test]
async fn a_placement_removed_while_retired_refuses_the_restore() {
    let (plane, fake, _dir, _, retired) = a_retired_published_client().await;
    plane.continuity.as_ref().unwrap().enroll(&id("globex"));
    fake.seed_placements(DesiredRevision::new("pl-2"), vec![placement("acme")]);

    let (status, body) = restore(&plane, "globex", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "restore_placements_not_retained");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("last published at revision 1"), "{message}");
    assert_eq!(revision_of(&plane, "globex").await, retired);
    assert_eq!(continuity_events(&plane), [], "refused before the reservation");
}

#[tokio::test]
async fn retained_placements_are_not_assumed_before_any_pass_has_published() {
    let dir = TempDir::new("restore-unverified");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    plane.continuity.as_ref().unwrap().enroll(&id("acme"));
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(DesiredRevision::new("pl-1"), vec![]);
    let retired = tombstoned(&plane).await;

    let (status, body) = restore(&plane, "acme", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "restore_placements_unverified");
    assert_eq!(revision_of(&plane, "acme").await, retired);
}

#[tokio::test]
async fn each_continuity_refusal_writes_nothing() {
    for (refusal, status, code) in [
        (
            ContinuityRefusal::Mismatch,
            StatusCode::CONFLICT,
            "lifecycle_continuity_mismatch",
        ),
        (
            ContinuityRefusal::Unavailable {
                detail: "a detail the browser must not see".to_owned(),
            },
            StatusCode::SERVICE_UNAVAILABLE,
            "lifecycle_continuity_unavailable",
        ),
    ] {
        let plane = control_plane();
        let continuity = plane.continuity.as_ref().unwrap();
        continuity.enroll(&id("acme"));
        continuity.refuse_with(refusal);
        let retired = tombstoned(&plane).await;
        let audited_before = audited(&plane, "acme").await;

        let response = send(
            &plane.router,
            as_operator("POST", "/api/clients/acme/restoration")
                .header("content-type", "application/json")
                .header("if-match", format!("\"{retired}\""))
                .body(Body::from(json!({"reason": "Contract renewed"}).to_string()))
                .unwrap(),
        )
        .await;
        let answered = response.status();
        let retry_after = response.headers().get(RETRY_AFTER).is_some();
        let body = body_of(response).await;

        assert_eq!(answered, status, "{body}");
        assert_eq!(body["error"]["code"], code);
        assert_eq!(retry_after, status == StatusCode::SERVICE_UNAVAILABLE);
        assert!(!body.to_string().contains("must not see"), "{body}");
        assert_eq!(revision_of(&plane, "acme").await, retired);
        assert_eq!(audited(&plane, "acme").await, audited_before);
    }
}

#[tokio::test]
async fn an_unenrolled_client_is_refused_rather_than_enrolled() {
    let plane = control_plane();
    let retired = tombstoned(&plane).await;

    let (status, body) = restore(&plane, "acme", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "lifecycle_continuity_unenrolled");
    assert_eq!(revision_of(&plane, "acme").await, retired);
    assert_eq!(continuity_events(&plane), []);
}

#[tokio::test]
async fn a_write_that_loses_its_revision_after_the_reservation_releases_it() {
    let plane = control_plane();
    let continuity = plane.continuity.as_ref().unwrap();
    continuity.enroll(&id("acme"));
    let retired = tombstoned(&plane).await;
    let concurrent = plane.repository.clone();
    let document = plane.repository.get(&id("acme")).await.unwrap().document;
    continuity.on_reserve(move || {
        concurrent.insert(&document).unwrap();
    });

    let (status, body) = restore(&plane, "acme", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "revision_conflict");
    assert_eq!(
        continuity_events(&plane),
        [
            ContinuityEvent::Reserved(id("acme"), ClientRevision::try_new(&retired).unwrap()),
            ContinuityEvent::Released(id("acme")),
        ]
    );
    let (_, after) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(after["lifecycle"]["state"], "tombstoned", "{after}");
    assert!(!audited(&plane, "acme")
        .await
        .contains(&"control_plane.audit.client_restored".to_owned()));
}

#[tokio::test]
async fn a_write_that_may_have_landed_leaves_its_reservation_pending() {
    let plane = control_plane();
    let continuity = plane.continuity.as_ref().unwrap();
    continuity.enroll(&id("acme"));
    let retired = tombstoned(&plane).await;
    let repository = plane.repository.clone();
    continuity.on_reserve(move || repository.set_unavailable(Some("the host timed out".to_owned())));

    let (status, body) = restore(&plane, "acme", &retired).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "repository_unavailable");
    assert_eq!(
        continuity_events(&plane),
        [ContinuityEvent::Reserved(
            id("acme"),
            ClientRevision::try_new(&retired).unwrap()
        )],
        "not released over a write that may have landed"
    );
}

#[tokio::test]
async fn a_failed_completion_keeps_the_written_restoration_and_its_reservation_pending() {
    let plane = control_plane();
    let continuity = plane.continuity.as_ref().unwrap();
    continuity.enroll(&id("acme"));
    continuity.fail_completions();
    let retired = tombstoned(&plane).await;

    let (status, body) = restore(&plane, "acme", &retired).await;

    assert_eq!(status, StatusCode::OK, "the document was written: {body}");
    assert_eq!(body["lifecycle"]["state"], "incomplete");
    assert_eq!(
        continuity_events(&plane),
        [ContinuityEvent::Reserved(
            id("acme"),
            ClientRevision::try_new(&retired).unwrap()
        )],
        "never completed, never released"
    );
    assert!(audited(&plane, "acme")
        .await
        .contains(&"control_plane.audit.client_restored".to_owned()));
}

// Identity comes back only through D01-19b/D01-15's readiness rules.

#[tokio::test]
async fn a_restored_client_is_identity_ready_only_once_converged_with_a_consistent_projection() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    plane.continuity.as_ref().unwrap().enroll(&id("newco"));
    let callbacks = provider_callbacks(&provider);
    let current = revision_of(&plane, "newco").await;
    let (_, retired) = tombstone(&plane.router, "newco", &current).await;
    let retired = retired["revision"].as_str().unwrap().to_owned();
    settled(&plane, &retired).await;

    let (status, restored) = restore(&plane, "newco", &retired).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    let revision = restored["revision"].as_str().unwrap();
    assert!(identity_unmet(&plane.router).await, "pending at the new revision");

    settled(&plane, revision).await;
    assert!(
        !identity_unmet(&plane.router).await,
        "applied at the restored revision with a consistent projection"
    );
    assert_eq!(provider_callbacks(&provider), callbacks, "nothing was re-created");
}

#[tokio::test]
async fn a_restored_client_whose_projection_diverged_in_git_stays_identity_unmet() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    plane.continuity.as_ref().unwrap().enroll(&id("newco"));
    let callbacks = provider_callbacks(&provider);
    let current = revision_of(&plane, "newco").await;
    let (_, retired) = tombstone(&plane.router, "newco", &current).await;
    settled(&plane, retired["revision"].as_str().unwrap()).await;
    let edited = hand_edit_in_git(&plane, |text| {
        text.replace(
            "- https://newco.example.com/analytics/callback",
            "- https://newco.example.com/analytics/callback\n        - https://evil.example.com/callback",
        )
    })
    .await;

    let (status, restored) = restore(&plane, "newco", &edited).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    settled(&plane, restored["revision"].as_str().unwrap()).await;
    sweep(&plane.router).await;

    let (_, client) = get(&plane.router, "/api/clients/newco").await;
    let identity = client["lifecycle"]["unmet"]
        .as_array()
        .unwrap()
        .iter()
        .find(|unmet| unmet["condition"] == "identity")
        .expect("identity stays unmet");
    assert!(
        identity["detail"].as_str().unwrap().contains("projection"),
        "{client}"
    );
    assert_eq!(
        provider_callbacks(&provider),
        callbacks,
        "the diverged entry was withheld, not written"
    );
}
