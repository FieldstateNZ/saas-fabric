//! Restoring a tombstoned client, and the continuity admission every
//! identity side effect passes, through the real router (D01-14; ADR 0021,
//! "Restoring a tombstone"; #108 comments 6113749198 item 3, 6119161317
//! item 2 and 6120741272).
//!
//! Without a continuity store, which is every deployment today, a restore is
//! held and no client is admitted. With the test-only store: an explicit
//! operator write at the tombstoned revision, refused for a confirmed
//! deletion or unretained placements, reserved before the document is
//! written, completed after, recoverable by sending it again, audited once
//! confirmed; and a lifecycle Git rewrote, by revert, forgery, erasure or
//! an earlier blob, is held on every read, pass and publication.

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
use fabric_client_model::{ClientDocument, ClientId, ClientRevision, LifecycleRecord};
use fabric_control_plane::testing::ContinuityEvent;
use fabric_control_plane::{
    AuditStream, AuditTrail, ChangeContext, ClientRepository, ContinuityRefusal, TransitionKind,
};
use fabric_core::{BindingRevision, DataSourceId};
use fabric_platform_management::{DesiredRevision, PlacementState};
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::ReconciliationOutcome;
use http::header::{ETAG, RETRY_AFTER};
use http::StatusCode;
use serde_json::{json, Value};
use support::lifecycle::{
    conditions, confirm_deletion, declared_data_source, get, placement, post,
    publish_one_application_with_a_resource, sink, tenants_document, tombstone, GLOBEX,
};
use support::newco::{
    converged_on, identity_unmet, newco, provider_callbacks, reassert, save_product_on, settled, sweep,
};
use support::platform_fixture::{platform_binding, FakeRepository};
use support::projected::{hand_edit_in_git, newco_with_a_projected_client};
use support::{
    as_operator, control_plane, control_plane_with_no_continuity, control_plane_with_publication,
    json as body_of, restarted, send, TempDir, TestControlPlane, OPERATOR,
};

fn id(value: &str) -> ClientId {
    ClientId::try_new(value).unwrap()
}

fn revision(value: &str) -> ClientRevision {
    ClientRevision::try_new(value).unwrap()
}

async fn restore(plane: &TestControlPlane, client: &str, at: &str) -> (StatusCode, Value) {
    post(
        &plane.router,
        &format!("/api/clients/{client}/restoration"),
        Some(at),
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

fn restored_audits(audit: &[String]) -> usize {
    audit
        .iter()
        .filter(|event| *event == "control_plane.audit.client_restored")
        .count()
}

/// Every store event about a restore, in order.
fn restore_events(plane: &TestControlPlane) -> Vec<ContinuityEvent> {
    plane
        .continuity
        .as_ref()
        .unwrap()
        .events()
        .into_iter()
        .filter(|event| match event {
            ContinuityEvent::Reserved(_, kind, _) => *kind == TransitionKind::Restore,
            _ => true,
        })
        .skip_while(|event| !matches!(event, ContinuityEvent::Reserved(..)))
        .collect()
}

/// Whether no provider call reached `newco`'s realm. The `acme` fixture is
/// admitted and converges beside it.
fn no_call_names_newco(provider: &FakeIdentityProvider) -> bool {
    provider.calls().iter().all(|call| !call.contains("newco"))
}

fn store(plane: &TestControlPlane) -> &fabric_control_plane::testing::InMemoryLifecycleContinuity {
    plane.continuity.as_ref().unwrap()
}

/// `acme` tombstoned in `plane`, and the revision the tombstone produced.
async fn tombstoned(plane: &TestControlPlane) -> String {
    let (status, body) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["revision"].as_str().unwrap().to_owned()
}

/// Writes `document` over `client` as a Git commit would.
async fn commit(plane: &TestControlPlane, client: &str, document: &ClientDocument) -> String {
    let current = plane.repository.get(&id(client)).await.unwrap();
    let change = ChangeContext {
        requested_by: "someone-in-git".into(),
        summary: "hand edit".into(),
    };
    plane
        .repository
        .update(&id(client), document, &current.revision, &change)
        .await
        .unwrap()
        .as_str()
        .to_owned()
}

async fn continuity_of(plane: &TestControlPlane, client: &str) -> Value {
    get(&plane.router, &format!("/api/clients/{client}")).await.1["lifecycle"]["continuity"].clone()
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
    plane.enroll("globex").await;
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

// No continuity store: every deployment today.

#[tokio::test]
async fn without_a_continuity_store_a_restore_is_held_and_writes_nothing() {
    let plane = control_plane_with_no_continuity(None, None, None);
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
async fn without_a_continuity_store_the_hold_comes_after_the_revision_and_lifecycle_checks() {
    let plane = control_plane_with_no_continuity(None, None, None);

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

#[tokio::test]
async fn without_a_continuity_store_no_identity_is_admitted_and_nothing_reaches_the_provider() {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_no_continuity(None, None, Some(provider.clone()));

    // Creation stages desired intent, and nothing converges it.
    let revision = newco_with_a_projected_client(&plane.router).await;
    sweep(&plane.router).await;
    assert!(no_call_names_newco(&provider), "{:?}", provider.calls());
    let continuity = continuity_of(&plane, "newco").await;
    assert_eq!(continuity["state"], "held", "{continuity}");
    assert_eq!(continuity["reason"], "noStore", "{continuity}");
    assert!(identity_unmet(&plane.router).await);

    // Re-assertion is held before any provider call.
    let response = reassert(&plane.router).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get(RETRY_AFTER).is_none());
    assert_eq!(
        body_of(response).await["error"]["code"],
        "lifecycle_continuity_unavailable"
    );
    assert!(provider.calls().is_empty());

    // Retiring only restricts, so it is still written.
    let (status, body) = tombstone(&plane.router, "newco", &revision).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lifecycle"]["state"], "tombstoned");
}

// The write, with the test-only continuity store.

#[tokio::test]
async fn a_restore_reverses_the_tombstone_forward_and_leaves_the_client_saved_intent() {
    let (plane, fake, dir, before, retired) = a_retired_published_client().await;
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
    let at = restored["revision"].as_str().unwrap().to_owned();
    assert_eq!(etag, format!("\"{at}\""));
    assert_ne!(at, retired);
    assert_ne!(at, before);

    // Saved intent again, admitted: identity is pending at the new revision.
    let lifecycle = &restored["lifecycle"];
    assert_eq!(lifecycle["state"], "incomplete", "{restored}");
    assert_eq!(conditions(lifecycle), ["identity"], "{restored}");
    assert_eq!(
        lifecycle["unmet"][0]["detail"],
        "identity has not been applied at this revision yet"
    );
    assert_eq!(lifecycle["continuity"]["state"], "admitted", "{restored}");
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
        restore_events(&plane),
        [
            ContinuityEvent::Reserved(id("globex"), TransitionKind::Restore, Some(revision(&retired))),
            ContinuityEvent::Completed(id("globex"), revision(&at)),
        ]
    );

    // Audited through the durable trail once confirmed, naming the tombstone.
    let stored = plane
        .audit
        .read(&AuditStream::Client(id("globex")))
        .await
        .unwrap();
    let entry = stored.entries.last().unwrap().record();
    assert_eq!(entry.event(), "control_plane.audit.client_restored");
    assert_eq!(entry.requested_by(), OPERATOR);
    assert_eq!(entry.field("revision"), Some(at.as_str()));
    assert!(entry.field("tombstonedAt").is_some());
    assert!(stored.verify().is_ok());

    // Nothing else moved: data intent, identity, secrets reference,
    // placements and the published tenants document.
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
        revision(&at),
        &ReconciliationOutcome::converged(),
        2,
    );
    let (_, ready) = get(&plane.router, "/api/clients/globex").await;
    assert_eq!(ready["lifecycle"]["state"], "ready", "{ready}");
}

#[tokio::test]
async fn a_restored_client_can_be_retired_again_in_the_same_second_and_its_history_only_grows() {
    let plane = control_plane();
    let first = tombstoned(&plane).await;
    let (_, restored) = restore(&plane, "acme", &first).await;

    let (status, again) = tombstone(&plane.router, "acme", restored["revision"].as_str().unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["lifecycle"]["state"], "tombstoned");
    assert_eq!(again["lifecycle"]["restorations"].as_array().unwrap().len(), 1);

    let (status, twice) = restore(&plane, "acme", again["revision"].as_str().unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{twice}");
    let history = twice["lifecycle"]["restorations"].as_array().unwrap();
    assert_eq!(history.len(), 2);
    // The fixture clock never moves: every transition is in one second, and
    // the store's sequence, not the clock, orders them.
    let seconds: Vec<&Value> = history
        .iter()
        .flat_map(|entry| [&entry["tombstone"]["at"], &entry["at"]])
        .collect();
    assert!(seconds.windows(2).all(|pair| pair[0] == pair[1]), "{history:?}");
    assert_eq!(twice["lifecycle"]["continuity"]["state"], "admitted", "{twice}");
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
    let retired = tombstoned(&plane).await;
    let (_, confirmed) = confirm_deletion(&plane.router, "acme", &retired).await;
    let at = confirmed["revision"].as_str().unwrap();

    let (status, body) = restore(&plane, "acme", at).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "deletion_confirmed_not_restorable");
    assert_eq!(revision_of(&plane, "acme").await, at);
    assert_eq!(restore_events(&plane), []);
}

#[tokio::test]
async fn a_restore_needs_the_current_revision_an_operator_and_a_reason() {
    let plane = control_plane();
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
    assert_eq!(restore_events(&plane), []);
}

#[tokio::test]
async fn a_placement_removed_while_retired_refuses_the_restore() {
    let (plane, fake, _dir, _, retired) = a_retired_published_client().await;
    fake.seed_placements(DesiredRevision::new("pl-2"), vec![placement("acme")]);

    let (status, body) = restore(&plane, "globex", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "restore_placements_not_retained");
    assert_eq!(revision_of(&plane, "globex").await, retired);
    assert_eq!(restore_events(&plane), [], "refused before the reservation");
}

#[tokio::test]
async fn a_replaced_binding_masked_by_a_raised_revision_refuses_the_restore() {
    let (plane, fake, _dir, _, retired) = a_retired_published_client().await;
    let mut replaced = placement("globex");
    replaced.data_source = DataSourceId::try_new("another-postgres").unwrap();
    replaced.revision = BindingRevision::new(5);
    fake.seed_placements(DesiredRevision::new("pl-2"), vec![placement("acme"), replaced]);

    let (status, body) = restore(&plane, "globex", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "restore_placements_not_retained");
    assert_eq!(revision_of(&plane, "globex").await, retired);
}

#[tokio::test]
async fn an_unreadable_held_publication_refuses_the_restore() {
    let (plane, _fake, dir, _, retired) = a_retired_published_client().await;
    std::fs::write(dir.path().join("tenants.json"), "not json").unwrap();

    let (status, body) = restore(&plane, "globex", &retired).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "placements_unavailable");
    assert_eq!(revision_of(&plane, "globex").await, retired);
}

#[tokio::test]
async fn a_target_that_never_accepted_a_publication_is_not_evidence_of_retention() {
    let dir = TempDir::new("restore-unverified");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(DesiredRevision::new("pl-1"), vec![]);
    let retired = tombstoned(&plane).await;

    let (status, body) = restore(&plane, "acme", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "restore_placements_unverified");
    assert_eq!(revision_of(&plane, "acme").await, retired);
}

#[tokio::test]
async fn each_continuity_refusal_writes_nothing_and_shows_no_store_detail() {
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
        let retired = tombstoned(&plane).await;
        store(&plane).refuse_with(refusal);
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
async fn missing_or_unavailable_history_holds_the_client_and_refuses_the_restore() {
    let plane = control_plane();
    let retired = tombstoned(&plane).await;
    plane
        .repository
        .insert(&ClientDocument::parse(GLOBEX).unwrap())
        .unwrap();

    let unenrolled = continuity_of(&plane, "globex").await;
    assert_eq!(unenrolled["reason"], "unenrolled", "{unenrolled}");

    store(&plane).set_unavailable(true);
    let unavailable = continuity_of(&plane, "acme").await;
    assert_eq!(unavailable["reason"], "unavailable", "{unavailable}");
    let (status, body) = restore(&plane, "acme", &retired).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "lifecycle_continuity_unavailable");
    assert_eq!(revision_of(&plane, "acme").await, retired);
}

// Partial success, recovery and conflicting reservations.

#[tokio::test]
async fn a_write_that_loses_its_revision_after_the_reservation_releases_it() {
    let plane = control_plane();
    let retired = tombstoned(&plane).await;
    let concurrent = plane.repository.clone();
    let document = plane.repository.get(&id("acme")).await.unwrap().document;
    store(&plane).on_reserve(move || {
        concurrent.insert(&document).unwrap();
    });

    let (status, body) = restore(&plane, "acme", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "revision_conflict");
    assert_eq!(
        restore_events(&plane),
        [
            ContinuityEvent::Reserved(id("acme"), TransitionKind::Restore, Some(revision(&retired))),
            ContinuityEvent::Released(id("acme")),
        ]
    );
    let (_, after) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(after["lifecycle"]["state"], "tombstoned", "{after}");
    assert_eq!(restored_audits(&audited(&plane, "acme").await), 0);
}

#[tokio::test]
async fn a_write_that_may_have_landed_stays_pending_and_the_same_request_recovers_it() {
    let plane = control_plane();
    let retired = tombstoned(&plane).await;
    let repository = plane.repository.clone();
    store(&plane).on_reserve(move || repository.set_unavailable(Some("the host timed out".to_owned())));

    let (status, body) = restore(&plane, "acme", &retired).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "repository_unavailable");
    plane.repository.set_unavailable(None);
    store(&plane).on_reserve(|| {});
    let pending = continuity_of(&plane, "acme").await;
    assert_eq!(
        pending["reason"], "pending",
        "not released over a write that may have landed"
    );

    // The write never landed: the document is still at the reviewed
    // revision, so the store hands back the same reservation.
    let (status, body) = restore(&plane, "acme", &retired).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lifecycle"]["continuity"]["state"], "admitted", "{body}");
    assert_eq!(restored_audits(&audited(&plane, "acme").await), 1);
}

#[tokio::test]
async fn a_failed_completion_stays_pending_across_a_restart_until_the_same_request_recovers_it() {
    let plane = control_plane();
    let retired = tombstoned(&plane).await;
    store(&plane).fail_completions(true);

    let (status, body) = restore(&plane, "acme", &retired).await;

    // The document is written, and the response says the transition is not
    // confirmed rather than reporting it complete.
    assert_eq!(status, StatusCode::OK, "{body}");
    let landed = body["revision"].as_str().unwrap().to_owned();
    assert_eq!(body["lifecycle"]["continuity"]["reason"], "pending", "{body}");
    assert!(conditions(&body["lifecycle"]).contains(&"identity"));
    assert_eq!(
        restored_audits(&audited(&plane, "acme").await),
        0,
        "audited once confirmed"
    );

    // A restart keeps the store and the document, so it is still pending.
    store(&plane).fail_completions(false);
    let after_restart = restarted(&plane, None);
    let pending = continuity_of(&after_restart, "acme").await;
    assert_eq!(pending["reason"], "pending", "{pending}");

    // Sending the same request again completes it at the landed revision,
    // without writing the document again.
    let (status, body) = restore(&after_restart, "acme", &retired).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], landed.as_str());
    assert_eq!(body["lifecycle"]["continuity"]["state"], "admitted", "{body}");
    assert_eq!(revision_of(&after_restart, "acme").await, landed);
    assert_eq!(
        restore_events(&after_restart).last(),
        Some(&ContinuityEvent::Completed(id("acme"), revision(&landed)))
    );
}

#[tokio::test]
async fn a_conflicting_transition_while_one_is_pending_is_refused_and_writes_nothing() {
    let plane = control_plane();
    let retired = tombstoned(&plane).await;
    store(&plane).fail_completions(true);
    let (_, body) = restore(&plane, "acme", &retired).await;
    let landed = body["revision"].as_str().unwrap().to_owned();
    store(&plane).fail_completions(false);

    let (status, body) = tombstone(&plane.router, "acme", &landed).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "lifecycle_continuity_pending");
    assert_eq!(revision_of(&plane, "acme").await, landed);
}

// A lifecycle rewritten in Git is held everywhere.

#[tokio::test]
async fn a_restoration_forged_in_git_is_held_and_publishes_nothing_new() {
    let (plane, _fake, dir, _, _) = a_retired_published_client().await;
    let stored = plane.repository.get(&id("globex")).await.unwrap();
    let at = stored.document.lifecycle().tombstone.as_ref().unwrap().at;
    let forged = stored
        .document
        .lifecycle()
        .restore(at, "someone-in-git", "forged")
        .unwrap();
    commit(
        &plane,
        "globex",
        &stored.document.with_lifecycle(&forged).unwrap(),
    )
    .await;
    let held_documents = tenants_document(&dir);

    let continuity = continuity_of(&plane, "globex").await;
    assert_eq!(continuity["reason"], "mismatch", "{continuity}");
    let (_, pass) = post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    // This sink publishes no issuer registry, so the pass decides nothing
    // that needs an admission; `client_document_revert.rs` and
    // `issuer_registry.rs` cover the held registry.
    assert_eq!(pass["lastPass"]["outcome"], "unchanged", "{pass}");
    assert!(
        conditions(&get(&plane.router, "/api/clients/globex").await.1["lifecycle"]).contains(&"identity")
    );
    assert_eq!(tenants_document(&dir), held_documents);
    let (status, body) = restore(&plane, "globex", &revision_of(&plane, "globex").await).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "client_not_tombstoned");
}

#[tokio::test]
async fn an_erased_history_or_an_earlier_blob_is_held() {
    let plane = control_plane();
    let before_tombstone = plane.repository.get(&id("acme")).await.unwrap().document;
    let retired = tombstoned(&plane).await;
    let (_, restored) = restore(&plane, "acme", &retired).await;
    assert_eq!(restored["lifecycle"]["continuity"]["state"], "admitted");
    let restored_document = plane.repository.get(&id("acme")).await.unwrap().document;

    let erased = restored_document
        .with_lifecycle(&LifecycleRecord::default())
        .unwrap();
    commit(&plane, "acme", &erased).await;
    assert_eq!(continuity_of(&plane, "acme").await["reason"], "mismatch");

    // The exact bytes from before the tombstone: what a revert to an earlier
    // blob reads as.
    commit(&plane, "acme", &before_tombstone).await;
    assert_eq!(continuity_of(&plane, "acme").await["reason"], "mismatch");

    commit(&plane, "acme", &restored_document).await;
    assert_eq!(continuity_of(&plane, "acme").await["state"], "admitted");
}

#[tokio::test]
async fn a_reverted_tombstone_is_held_by_the_sweep_the_write_after_it_and_reassertion() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    let before_tombstone = plane.repository.get(&newco()).await.unwrap().document;
    let current = revision_of(&plane, "newco").await;
    let (_, retired) = tombstone(&plane.router, "newco", &current).await;
    settled(&plane, retired["revision"].as_str().unwrap()).await;
    commit(&plane, "newco", &before_tombstone).await;
    provider.clear_calls();

    // A sweep, and the convergence a product save starts after it writes.
    sweep(&plane.router).await;
    save_product_on(&plane.router, &["newco.example.com"], None).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(no_call_names_newco(&provider), "{:?}", provider.calls());
    assert!(identity_unmet(&plane.router).await);

    let response = reassert(&plane.router).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_of(response).await["error"]["code"],
        "lifecycle_continuity_mismatch"
    );
    assert!(no_call_names_newco(&provider), "{:?}", provider.calls());
}

#[tokio::test]
async fn a_new_identity_is_admitted_and_a_retired_one_created_again_is_held() {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = support::control_plane_with_identity_provider(provider.clone());
    let revision = newco_with_a_projected_client(&plane.router).await;
    settled(&plane, &revision).await;
    assert_eq!(continuity_of(&plane, "newco").await["state"], "admitted");
    assert!(!identity_unmet(&plane.router).await);

    let (_, retired) = tombstone(&plane.router, "newco", &revision).await;
    settled(&plane, retired["revision"].as_str().unwrap()).await;
    assert!(plane.repository.remove_by_hand(&newco()));
    provider.clear_calls();

    let created = send(
        &plane.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":{
                    "displayName":"Newco","legalName":"Newco Ltd","region":"NZ","timezone":"Pacific/Auckland",
                    "hosts":["newco.example.com"],"configuration":{},"applications":[]}})
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED, "intent is still staged");
    sweep(&plane.router).await;

    let continuity = continuity_of(&plane, "newco").await;
    assert_eq!(continuity["state"], "held", "{continuity}");
    assert!(identity_unmet(&plane.router).await);
    assert!(no_call_names_newco(&provider), "{:?}", provider.calls());
}

// Identity comes back only through D01-19b/D01-15's readiness rules.

#[tokio::test]
async fn a_restored_client_is_identity_ready_only_once_converged_with_a_consistent_projection() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    let callbacks = provider_callbacks(&provider);
    let current = revision_of(&plane, "newco").await;
    let (_, retired) = tombstone(&plane.router, "newco", &current).await;
    let retired = retired["revision"].as_str().unwrap().to_owned();
    settled(&plane, &retired).await;

    let (status, restored) = restore(&plane, "newco", &retired).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert!(identity_unmet(&plane.router).await, "pending at the new revision");

    settled(&plane, restored["revision"].as_str().unwrap()).await;
    assert!(
        !identity_unmet(&plane.router).await,
        "applied at the restored revision with a consistent projection"
    );
    assert_eq!(provider_callbacks(&provider), callbacks, "nothing was re-created");
}

#[tokio::test]
async fn a_restored_client_whose_projection_diverged_in_git_stays_identity_unmet() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
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
