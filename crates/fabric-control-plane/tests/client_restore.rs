//! Restoring a tombstoned client, and the continuity admission every
//! identity side effect passes, through the real router (D01-14; ADR 0021,
//! "Restoring a tombstone"; #108 comments 6113749198 item 3, 6119161317
//! item 2 and 6120741272; review 5489320331).
//!
//! Without a continuity store, which is every deployment today, a restore is
//! held and no client is admitted. With the test-only store: an explicit
//! operator write at the tombstoned revision, refused for a confirmed
//! deletion or unretained or unknown placements, reserved as one frozen
//! operation before the document is written, completed after, replayed or
//! recovered only by the same request with its preconditions checked again,
//! audited once confirmed; and a lifecycle, identity instance or identity
//! context Git rewrote is held on every read, pass and publication.

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
    AuditStream, AuditTrail, ChangeContext, ClientRepository, ContinuityRefusal, LifecycleContinuity,
    TransitionKind,
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
use support::newco::{identity_unmet, newco, provider_callbacks, reassert, save_product_on, settled, sweep};
use support::platform_fixture::{platform_binding, platform_binding_on, FakeRepository};
use support::projected::{hand_edit_in_git, newco_with_a_projected_client};
use support::{
    as_operator, control_plane_with_no_continuity, control_plane_with_publication,
    control_plane_with_publication_and_identity_provider, json as body_of, restarted, send, TempDir,
    TestControlPlane, OPERATOR,
};

fn id(value: &str) -> ClientId {
    ClientId::try_new(value).unwrap()
}

fn revision(value: &str) -> ClientRevision {
    ClientRevision::try_new(value).unwrap()
}

/// A plane with a platform, a publication target holding an accepted
/// publication (`globex`, placed and published), and optionally a continuity
/// store and an identity provider. `acme` is never placed, so the target's
/// history establishes that it was never published.
struct Published {
    plane: TestControlPlane,
    fake: Arc<FakeRepository>,
    dir: TempDir,
    globex: ClientRevision,
}

impl Published {
    async fn new(continuity: bool, provider: Option<Arc<FakeIdentityProvider>>) -> Self {
        Self::with_catalogue(continuity, provider, false).await
    }

    /// As [`Self::new`]; with `newco`, the catalogue is `newco`'s, created
    /// with one projected client and converged before the pass.
    async fn with_catalogue(
        continuity: bool,
        provider: Option<Arc<FakeIdentityProvider>>,
        newco: bool,
    ) -> Self {
        let dir = TempDir::new("restore");
        let (platform, fake) = platform_binding().await;
        let plane = match (continuity, provider) {
            (false, provider) => control_plane_with_no_continuity(Some(platform), Some(sink(&dir)), provider),
            (true, None) => control_plane_with_publication(platform, sink(&dir)),
            (true, Some(provider)) => {
                control_plane_with_publication_and_identity_provider(platform, sink(&dir), provider)
            }
        };
        let mut globex = plane
            .repository
            .insert(&ClientDocument::parse(GLOBEX).unwrap())
            .unwrap();
        if continuity {
            globex = plane.enroll("globex").await;
        }
        fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
        fake.seed_placements(DesiredRevision::new("pl-1"), vec![placement("globex")]);
        plane.statuses.record(
            &id("globex"),
            globex.clone(),
            &ReconciliationOutcome::converged(),
            1,
        );
        if newco {
            let at = newco_with_a_projected_client(&plane.router).await;
            settled(&plane, &at).await;
        }
        publish_one_application_with_a_resource(&plane.router).await;
        let (status, pass) = post(&plane.router, "/api/platform/publication", None, &json!({})).await;
        assert_eq!(status, StatusCode::OK, "{pass}");
        assert_eq!(pass["lastPass"]["outcome"], "published", "{pass}");
        Self {
            plane,
            fake,
            dir,
            globex,
        }
    }

    /// The same repository, platform and target, and the same continuity
    /// store, after a restart.
    async fn restarted(&self) -> TestControlPlane {
        let platform = platform_binding_on(Arc::clone(&self.fake)).await;
        restarted(&self.plane, None, Some(platform), Some(sink(&self.dir)))
    }
}

async fn restore(plane: &TestControlPlane, client: &str, at: &str) -> (StatusCode, Value) {
    restore_because(plane, client, at, "Contract renewed").await
}

async fn restore_because(
    plane: &TestControlPlane,
    client: &str,
    at: &str,
    reason: &str,
) -> (StatusCode, Value) {
    post(
        &plane.router,
        &format!("/api/clients/{client}/restoration"),
        Some(at),
        &json!({ "reason": reason }),
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

/// Every store event from the first restore reservation on.
fn restore_events(plane: &TestControlPlane) -> Vec<ContinuityEvent> {
    store(plane)
        .events()
        .into_iter()
        .skip_while(|event| !matches!(event, ContinuityEvent::Reserved(_, TransitionKind::Restore, _)))
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

/// `client` tombstoned at `at`, and the revision the tombstone produced.
async fn tombstoned_at(plane: &TestControlPlane, client: &str, at: &str) -> String {
    let (status, body) = tombstone(&plane.router, client, at).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["revision"].as_str().unwrap().to_owned()
}

async fn tombstoned(plane: &TestControlPlane) -> String {
    tombstoned_at(plane, "acme", plane.revision.as_str()).await
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

/// `newco` created with one projected client and converged, beside a
/// published environment.
async fn converged_newco() -> (Published, Arc<FakeIdentityProvider>) {
    let provider = Arc::new(FakeIdentityProvider::new());
    let published = Published::with_catalogue(true, Some(provider.clone()), true).await;
    (published, provider)
}

async fn create_newco(plane: &TestControlPlane, display_name: &str) -> StatusCode {
    send(
        &plane.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":{
                    "displayName":display_name,"legalName":"Newco Ltd","region":"NZ",
                    "timezone":"Pacific/Auckland","hosts":["newco.example.com"],"configuration":{},
                    "applications":[]}})
                .to_string(),
            ))
            .unwrap(),
    )
    .await
    .status()
}

// No continuity store: every deployment today.

#[tokio::test]
async fn without_a_continuity_store_a_restore_is_held_and_writes_nothing() {
    let Published { plane, dir, .. } = Published::new(false, None).await;
    let retired = tombstoned(&plane).await;
    let audited_before = audited(&plane, "acme").await;
    let documents = tenants_document(&dir);

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
    assert_eq!(tenants_document(&dir), documents);
}

#[tokio::test]
async fn without_a_continuity_store_the_hold_comes_after_the_revision_lifecycle_and_placement_checks() {
    let unplaced = control_plane_with_no_continuity(None, None, None);
    let retired = tombstoned(&unplaced).await;
    let (status, body) = restore(&unplaced, "acme", &retired).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"]["code"], "restore_placements_unverified",
        "no platform is not evidence of never published"
    );

    let Published { plane, dir: _dir, .. } = Published::new(false, None).await;
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

    let at = newco_with_a_projected_client(&plane.router).await;
    sweep(&plane.router).await;
    assert!(provider.calls().is_empty(), "{:?}", provider.calls());
    let continuity = continuity_of(&plane, "newco").await;
    assert_eq!(continuity["state"], "held", "{continuity}");
    assert_eq!(continuity["reason"], "noStore", "{continuity}");
    assert!(identity_unmet(&plane.router).await);

    let response = reassert(&plane.router).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(response.headers().get(RETRY_AFTER).is_none());
    assert_eq!(
        body_of(response).await["error"]["code"],
        "lifecycle_continuity_unavailable"
    );
    assert!(provider.calls().is_empty());

    // Retiring only restricts, so it is still staged; it confers nothing.
    let (status, body) = tombstone(&plane.router, "newco", &at).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lifecycle"]["state"], "tombstoned");
    assert_eq!(body["lifecycle"]["continuity"]["state"], "held");
}

// The write, with the test-only continuity store.

#[tokio::test]
async fn a_restore_reverses_the_tombstone_forward_and_leaves_the_client_saved_intent() {
    let Published {
        plane,
        fake,
        dir,
        globex: before,
    } = Published::new(true, None).await;
    let retired = tombstoned_at(&plane, "globex", before.as_str()).await;
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

    let at = restored["revision"].as_str().unwrap().to_owned();
    assert_eq!(etag, format!("\"{at}\""));
    assert_ne!(at, retired);
    assert_ne!(at, before.as_str());

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

    let history = &lifecycle["restorations"];
    assert_eq!(history.as_array().unwrap().len(), 1, "{restored}");
    assert_eq!(history[0]["tombstone"]["reason"], "Contract ended");
    assert_eq!(history[0]["tombstone"]["by"], OPERATOR);
    assert_eq!(history[0]["reason"], "Contract renewed");
    assert_eq!(history[0]["by"], OPERATOR);

    assert_eq!(
        restore_events(&plane),
        [
            ContinuityEvent::Reserved(id("globex"), TransitionKind::Restore, Some(revision(&retired))),
            ContinuityEvent::Completed(id("globex"), revision(&at)),
        ]
    );

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

    // Nothing else moved: data intent, identity, the identity instance,
    // secrets reference, placements and the published tenants document.
    let stored_after = plane.repository.get(&id("globex")).await.unwrap();
    let (was, is) = (stored_before.document.client(), stored_after.document.client());
    assert_eq!(was.data, is.data);
    assert_eq!(was.identity, is.identity);
    assert_eq!(was.secrets, is.secrets);
    assert_eq!(was.lifecycle.instance, is.lifecycle.instance);
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
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
    let first = tombstoned(&plane).await;
    let (_, restored) = restore(&plane, "acme", &first).await;

    let again = tombstoned_at(&plane, "acme", restored["revision"].as_str().unwrap()).await;
    let (status, twice) = restore(&plane, "acme", &again).await;
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
    let acme = listed["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["id"] == "acme")
        .unwrap();
    assert_eq!(
        acme["lifecycle"]["restorations"].as_array().unwrap().len(),
        2,
        "{listed}"
    );
}

// Refusals before any write.

#[tokio::test]
async fn a_deletion_confirmed_client_is_not_restored_and_nothing_is_reserved() {
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
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
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
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

    let (status, body) = restore_because(&plane, "acme", &retired, " ").await;
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
    let Published {
        plane,
        fake,
        globex,
        dir: _dir,
        ..
    } = Published::new(true, None).await;
    let retired = tombstoned_at(&plane, "globex", globex.as_str()).await;
    fake.seed_placements(DesiredRevision::new("pl-2"), vec![]);

    let (status, body) = restore(&plane, "globex", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "restore_placements_not_retained");
    assert_eq!(revision_of(&plane, "globex").await, retired);
    assert_eq!(restore_events(&plane), [], "refused before the reservation");
}

#[tokio::test]
async fn a_replaced_binding_masked_by_a_raised_revision_refuses_the_restore() {
    let Published {
        plane,
        fake,
        globex,
        dir: _dir,
        ..
    } = Published::new(true, None).await;
    let retired = tombstoned_at(&plane, "globex", globex.as_str()).await;
    let mut replaced = placement("globex");
    replaced.data_source = DataSourceId::try_new("another-postgres").unwrap();
    replaced.revision = BindingRevision::new(5);
    fake.seed_placements(DesiredRevision::new("pl-2"), vec![replaced]);

    let (status, body) = restore(&plane, "globex", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "restore_placements_not_retained");
    assert_eq!(revision_of(&plane, "globex").await, retired);
}

#[tokio::test]
async fn an_unreadable_held_publication_refuses_the_restore() {
    let Published {
        plane, dir, globex, ..
    } = Published::new(true, None).await;
    let retired = tombstoned_at(&plane, "globex", globex.as_str()).await;
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
        let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
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
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
    let retired = tombstoned(&plane).await;
    plane
        .repository
        .insert(&ClientDocument::parse(&GLOBEX.replace("globex", "initech")).unwrap())
        .unwrap();

    let unenrolled = continuity_of(&plane, "initech").await;
    assert_eq!(unenrolled["reason"], "unenrolled", "{unenrolled}");

    store(&plane).set_unavailable(true);
    let unavailable = continuity_of(&plane, "acme").await;
    assert_eq!(unavailable["reason"], "unavailable", "{unavailable}");
    let (status, body) = restore(&plane, "acme", &retired).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error"]["code"], "lifecycle_continuity_unavailable");
    assert_eq!(revision_of(&plane, "acme").await, retired);
}

// Partial success, replay, recovery and conflicting reservations.

#[tokio::test]
async fn a_write_that_loses_its_revision_after_the_reservation_releases_only_its_own() {
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
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
async fn a_write_that_may_have_landed_stays_pending_and_only_the_same_request_replays_it() {
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
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

    // A different intent at the same revision is not the frozen operation.
    let (status, body) = restore_because(&plane, "acme", &retired, "A different reason").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "lifecycle_continuity_pending");
    assert_eq!(revision_of(&plane, "acme").await, retired);

    // The same request replays the frozen transition exactly.
    let (status, body) = restore(&plane, "acme", &retired).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lifecycle"]["continuity"]["state"], "admitted", "{body}");
    assert_eq!(body["lifecycle"]["restorations"][0]["reason"], "Contract renewed");
    assert_eq!(restored_audits(&audited(&plane, "acme").await), 1);
}

#[tokio::test]
async fn a_failed_completion_stays_pending_across_a_restart_until_the_same_request_recovers_it() {
    let published = Published::new(true, None).await;
    let plane = &published.plane;
    let retired = tombstoned(plane).await;
    store(plane).fail_completions(true);

    let (status, body) = restore(plane, "acme", &retired).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let landed = body["revision"].as_str().unwrap().to_owned();
    assert_eq!(body["lifecycle"]["continuity"]["reason"], "pending", "{body}");
    assert!(conditions(&body["lifecycle"]).contains(&"identity"));
    assert_eq!(
        restored_audits(&audited(plane, "acme").await),
        0,
        "audited once confirmed"
    );

    store(plane).fail_completions(false);
    let after_restart = published.restarted().await;
    let pending = continuity_of(&after_restart, "acme").await;
    assert_eq!(pending["reason"], "pending", "{pending}");

    let (status, body) = restore(&after_restart, "acme", &retired).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], landed.as_str());
    assert_eq!(body["lifecycle"]["continuity"]["state"], "admitted", "{body}");
    assert_eq!(revision_of(&after_restart, "acme").await, landed);
    assert_eq!(
        restore_events(&after_restart).last(),
        Some(&ContinuityEvent::Completed(id("acme"), revision(&landed)))
    );
    assert_eq!(restored_audits(&audited(&after_restart, "acme").await), 1);
}

#[tokio::test]
async fn recovery_checks_the_retained_bindings_again_and_completes_nothing_when_they_moved() {
    let Published {
        plane,
        fake,
        globex,
        dir: _dir,
        ..
    } = Published::new(true, None).await;
    let retired = tombstoned_at(&plane, "globex", globex.as_str()).await;
    store(&plane).fail_completions(true);
    let (status, body) = restore(&plane, "globex", &retired).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let landed = body["revision"].as_str().unwrap().to_owned();
    store(&plane).fail_completions(false);
    let events = store(&plane).events();

    // The held binding is removed before the operator retries.
    fake.seed_placements(DesiredRevision::new("pl-2"), vec![]);
    let (status, body) = restore(&plane, "globex", &retired).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "restore_placements_not_retained");
    assert_eq!(store(&plane).events(), events, "nothing completed or reserved");
    assert_eq!(revision_of(&plane, "globex").await, landed, "nothing written");
    assert_eq!(continuity_of(&plane, "globex").await["reason"], "pending");
    assert_eq!(restored_audits(&audited(&plane, "globex").await), 0);
}

#[tokio::test]
async fn a_conflicting_transition_while_one_is_pending_is_refused_and_writes_nothing() {
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
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

// A lifecycle, instance or context rewritten outside the store is held.

#[tokio::test]
async fn a_restoration_forged_in_git_is_held_and_publishes_nothing_new() {
    let Published {
        plane, dir, globex, ..
    } = Published::new(true, None).await;
    tombstoned_at(&plane, "globex", globex.as_str()).await;
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
    assert!(
        conditions(&get(&plane.router, "/api/clients/globex").await.1["lifecycle"]).contains(&"identity")
    );
    post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    assert_eq!(tenants_document(&dir), held_documents);
    let (status, body) = restore(&plane, "globex", &revision_of(&plane, "globex").await).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "client_not_tombstoned");
}

#[tokio::test]
async fn an_erased_history_an_earlier_blob_or_another_realm_is_held() {
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
    let before_tombstone = plane.repository.get(&id("acme")).await.unwrap().document;
    let retired = tombstoned(&plane).await;
    let (_, restored) = restore(&plane, "acme", &retired).await;
    assert_eq!(restored["lifecycle"]["continuity"]["state"], "admitted");
    let restored_document = plane.repository.get(&id("acme")).await.unwrap().document;

    let erased = LifecycleRecord {
        instance: restored_document.lifecycle().instance.clone(),
        ..LifecycleRecord::default()
    };
    commit(
        &plane,
        "acme",
        &restored_document.with_lifecycle(&erased).unwrap(),
    )
    .await;
    assert_eq!(continuity_of(&plane, "acme").await["reason"], "mismatch");

    // The exact bytes from before the tombstone: a revert to an earlier blob.
    commit(&plane, "acme", &before_tombstone).await;
    assert_eq!(continuity_of(&plane, "acme").await["reason"], "mismatch");

    // The same lifecycle and instance under another realm is another
    // identity context.
    let moved = restored_document
        .render()
        .unwrap()
        .replace("realm: acme", "realm: acme-two");
    commit(&plane, "acme", &ClientDocument::parse(&moved).unwrap()).await;
    assert_eq!(continuity_of(&plane, "acme").await["reason"], "mismatch");

    commit(&plane, "acme", &restored_document).await;
    assert_eq!(continuity_of(&plane, "acme").await["state"], "admitted");
}

#[tokio::test]
async fn a_reverted_tombstone_is_held_by_the_sweep_the_write_after_it_and_reassertion() {
    let (published, provider) = converged_newco().await;
    let plane = &published.plane;
    let before_tombstone = plane.repository.get(&newco()).await.unwrap().document;
    let current = revision_of(plane, "newco").await;
    let retired = tombstoned_at(plane, "newco", &current).await;
    settled(plane, &retired).await;
    commit(plane, "newco", &before_tombstone).await;
    provider.clear_calls();

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
async fn a_client_id_created_again_is_held_whether_its_identity_was_active_or_retired() {
    for retire_first in [false, true] {
        let (published, provider) = converged_newco().await;
        let plane = &published.plane;
        assert_eq!(continuity_of(plane, "newco").await["state"], "admitted");
        if retire_first {
            let current = revision_of(plane, "newco").await;
            let retired = tombstoned_at(plane, "newco", &current).await;
            settled(plane, &retired).await;
        }
        assert!(plane.repository.remove_by_hand(&newco()));
        provider.clear_calls();

        // The replacement's configuration differs, and its reservation is
        // refused: intent is staged, with no identity instance.
        assert_eq!(
            create_newco(plane, "Newco Replacement").await,
            StatusCode::CREATED
        );
        sweep(&plane.router).await;

        let continuity = continuity_of(plane, "newco").await;
        assert_eq!(continuity["state"], "held", "{continuity}");
        assert_eq!(continuity["reason"], "resurrection", "{continuity}");
        assert!(identity_unmet(&plane.router).await);
        assert!(no_call_names_newco(&provider), "{:?}", provider.calls());
        let staged = plane.repository.get(&newco()).await.unwrap();
        assert_eq!(staged.document.lifecycle().instance, None);
    }
}

#[tokio::test]
async fn a_new_client_id_is_attested_and_admitted() {
    let (published, provider) = converged_newco().await;
    let plane = &published.plane;
    let stored = plane.repository.get(&newco()).await.unwrap();

    assert!(stored.document.lifecycle().instance.is_some());
    assert_eq!(continuity_of(plane, "newco").await["state"], "admitted");
    assert!(!identity_unmet(&plane.router).await);
    assert!(!no_call_names_newco(&provider), "it converged");
}

// Identity comes back only through D01-19b/D01-15's readiness rules.

#[tokio::test]
async fn a_restored_client_is_identity_ready_only_once_converged_with_a_consistent_projection() {
    let (published, provider) = converged_newco().await;
    let plane = &published.plane;
    let callbacks = provider_callbacks(&provider);
    let current = revision_of(plane, "newco").await;
    let retired = tombstoned_at(plane, "newco", &current).await;
    settled(plane, &retired).await;

    let (status, restored) = restore(plane, "newco", &retired).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    // The write marked identity pending at the new revision; the
    // convergence it started applies it there (the response may already
    // show either, so it is the settled state that is asserted).
    settled(plane, restored["revision"].as_str().unwrap()).await;
    assert!(
        !identity_unmet(&plane.router).await,
        "applied at the restored revision with a consistent projection"
    );
    assert_eq!(provider_callbacks(&provider), callbacks, "nothing was re-created");
}

#[tokio::test]
async fn a_restored_client_whose_projection_diverged_in_git_stays_identity_unmet() {
    let (published, provider) = converged_newco().await;
    let plane = &published.plane;
    let callbacks = provider_callbacks(&provider);
    let current = revision_of(plane, "newco").await;
    let retired = tombstoned_at(plane, "newco", &current).await;
    settled(plane, &retired).await;
    let edited = hand_edit_in_git(plane, |text| {
        text.replace(
            "- https://newco.example.com/analytics/callback",
            "- https://newco.example.com/analytics/callback\n        - https://evil.example.com/callback",
        )
    })
    .await;

    let (status, restored) = restore(plane, "newco", &edited).await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    settled(plane, restored["revision"].as_str().unwrap()).await;
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

// Review 5489623418: identical concurrent attempts.

/// Two identical restores reserve the same operation. One writes and its
/// completion fails; the other loses its CAS and releases its own attempt.
/// That release proves only that its own attempt wrote nothing, so the
/// operation stays pending, and the same request recovers it once.
#[tokio::test]
async fn a_losing_concurrent_attempt_cannot_clear_the_landed_attempts_pending_operation() {
    let Published { plane, dir: _dir, .. } = Published::new(true, None).await;
    let retired = tombstoned(&plane).await;
    store(&plane).reserve_concurrently();
    store(&plane).fail_completions(true);

    let (status, body) = restore(&plane, "acme", &retired).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let landed = body["revision"].as_str().unwrap().to_owned();
    let other = store(&plane).concurrent_attempt().expect("a second attempt");

    // What the losing attempt's service does after its CAS fails.
    store(&plane).release(&other).await;
    assert_eq!(continuity_of(&plane, "acme").await["reason"], "pending");
    store(&plane).release(&other).await;
    assert_eq!(
        continuity_of(&plane, "acme").await["reason"],
        "pending",
        "a repeated release is stale"
    );

    store(&plane).fail_completions(false);
    // A different intent is not the frozen operation, landed or not. (A
    // different operator is refused the same way; `port_tests.rs` proves it,
    // since this harness attributes every request to one operator.)
    let (status, body) = restore_because(&plane, "acme", &retired, "A different reason").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "lifecycle_continuity_pending");
    assert_eq!(continuity_of(&plane, "acme").await["reason"], "pending");

    let (status, body) = restore(&plane, "acme", &retired).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], landed.as_str());
    assert_eq!(body["lifecycle"]["continuity"]["state"], "admitted", "{body}");
    assert_eq!(restored_audits(&audited(&plane, "acme").await), 1);

    // Recovery is idempotent: the operation is complete, and a stale
    // release or completion from it touches nothing after.
    store(&plane).release(&other).await;
    assert_eq!(
        store(&plane).complete(&other, &revision(&landed)).await,
        Err(ContinuityRefusal::StaleCompletion)
    );
    assert_eq!(continuity_of(&plane, "acme").await["state"], "admitted");
}
