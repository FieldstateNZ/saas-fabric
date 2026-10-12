//! D01-19b, completed: a stored projected client that is not its own
//! projection is reported whenever the client is read, is withheld from
//! every sweep, and refuses a re-assertion.
//!
//! Each divergence is made the way a Git hand edit arrives (see
//! `support::projected::hand_edit_in_git`). The report is a fact about the
//! stored document: it names client and field, never a callback, and never
//! who edited Git.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::Router;
use fabric_client_model::{ClientId, ClientRevision, OidcClientId, RealmName};
use fabric_control_plane::{AuditStream, AuditTrail as _, ClientRepository as _};
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::{ObservedOidcClient, ReconciliationStatus};
use http::{header, Response, StatusCode};
use serde_json::{json, Value};
use support::projected::{hand_edit_in_git, newco_with_a_projected_client};
use support::{
    as_operator, control_plane, control_plane_with_identity_provider, entity_tag, send, TestControlPlane,
};

const CALLBACK: &str = "https://newco.example.com/analytics/callback";
const EXTRA: &str = "https://extra.example.com/analytics/callback";

fn newco() -> ClientId {
    ClientId::try_new("newco").unwrap()
}

async fn get(router: &Router, path: &str) -> Value {
    let response = send(router, as_operator("GET", path).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    support::json(response).await
}

/// The projection as every read reports it: the list, the overview and the
/// identity view, which must agree.
async fn reported_projection(router: &Router) -> Value {
    let list = get(router, "/api/clients").await;
    let listed = list["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["id"] == "newco")
        .unwrap()["projection"]
        .clone();
    let overview = get(router, "/api/clients/newco").await["projection"].clone();
    let identity = get(router, "/api/clients/newco/identity").await;
    let revision = identity["revision"].clone();
    assert_eq!(get(router, "/api/clients/newco").await["revision"], revision);
    let identity = identity["projection"].clone();
    assert_eq!(listed, overview);
    assert_eq!(overview, identity);
    identity
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

async fn reassert(router: &Router, revision: &str) -> Response<Body> {
    send(
        router,
        as_operator("POST", "/api/clients/newco/identity/reassertion")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

/// Waits for the convergence a write starts in the background to record
/// its outcome for `revision`, so it cannot land after a later step.
async fn settled(plane: &TestControlPlane, revision: &str) {
    let revision = ClientRevision::try_new(revision).unwrap();
    for _ in 0..500 {
        if plane.statuses.report(&newco()).is_some_and(|report| {
            report.revision == revision && report.status != ReconciliationStatus::Pending
        }) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the background convergence never recorded an outcome");
}

/// `newco` created with one projected client and converged into a fake
/// identity provider.
async fn converged_newco() -> (TestControlPlane, Arc<FakeIdentityProvider>) {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider(provider.clone());
    let revision = newco_with_a_projected_client(&plane.router).await;
    settled(&plane, &revision).await;
    (plane, provider)
}

fn analytics_in_provider(provider: &FakeIdentityProvider) -> ObservedOidcClient {
    provider
        .realm(&RealmName::try_new("newco").unwrap())
        .unwrap()
        .clients[&OidcClientId::try_new("analytics").unwrap()]
        .clone()
}

fn add_extra_callback(text: &str) -> String {
    text.replace(
        &format!("- {CALLBACK}"),
        &format!("- {CALLBACK}\n        - {EXTRA}"),
    )
}

async fn reasserted_audits(plane: &TestControlPlane) -> usize {
    let stored = plane.audit.read(&AuditStream::Client(newco())).await.unwrap();
    stored
        .entries
        .iter()
        .filter(|entry| entry.record().event() == "control_plane.audit.identity_projection_reasserted")
        .count()
}

#[tokio::test]
async fn a_consistent_projection_is_reported_as_consistent() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
}

#[tokio::test]
async fn a_divergence_made_in_git_is_reported_on_every_read_without_a_callback() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;
    hand_edit_in_git(&plane, |text| add_extra_callback(&text)).await;

    let projection = reported_projection(&plane.router).await;

    assert_eq!(
        projection,
        json!({"state": "diverged", "divergences": [{"client": "analytics", "field": "redirectUris"}]})
    );
    assert!(
        !projection.to_string().contains("https://"),
        "no callback is echoed"
    );
}

#[tokio::test]
async fn a_projected_entry_removed_in_git_is_reported_as_missing() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;
    hand_edit_in_git(&plane, |text| {
        let start = text.find("    clients:\n").unwrap();
        let end = text.find("  product:\n").unwrap();
        format!("{}    clients: []\n{}", &text[..start], &text[end..])
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "diverged", "divergences": [{"client": "analytics", "field": "missing"}]})
    );
}

#[tokio::test]
async fn a_sweep_withholds_a_diverged_entry_and_converges_the_rest() {
    let (plane, provider) = converged_newco().await;
    let before = analytics_in_provider(&provider);
    let realm = RealmName::try_new("newco").unwrap();
    let mut drifted = provider.realm(&realm).unwrap();
    drifted.roles.clear();
    provider.seed_realm(realm.clone(), drifted);
    hand_edit_in_git(&plane, |text| {
        add_extra_callback(&text).replace(
            "  product:\n",
            "    - id: manual\n      type: oidc\n      pkce: s256\n      redirect:\n        strategy: claimedHttps\n        uris:\n        - https://manual.example.com/callback\n  product:\n",
        )
    })
    .await;

    for _ in 0..2 {
        sweep(&plane.router).await;
    }

    assert_eq!(
        analytics_in_provider(&provider),
        before,
        "the Git edit is not converged into the identity provider"
    );
    let realm = provider.realm(&realm).unwrap();
    assert!(
        realm
            .clients
            .contains_key(&OidcClientId::try_new("manual").unwrap()),
        "a hand-declared client beside it still converges"
    );
    assert!(
        !realm.roles.is_empty(),
        "realm-role drift beside it is still corrected"
    );
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(identity["reconciliation"]["status"], "withheld", "{identity}");
    assert_eq!(identity["reconciliation"]["withheld"], json!(["analytics"]));
    let client = get(&plane.router, "/api/clients/newco").await;
    assert!(
        client["lifecycle"]["unmet"]
            .as_array()
            .unwrap()
            .iter()
            .any(|unmet| unmet["condition"] == "identity"),
        "a withheld client's identity is not in effect: {client}"
    );
}

#[tokio::test]
async fn a_sweep_does_not_correct_the_provider_onto_the_projection_either() {
    let (plane, provider) = converged_newco().await;
    let realm = RealmName::try_new("newco").unwrap();
    let mut state = provider.realm(&realm).unwrap();
    state
        .clients
        .get_mut(&OidcClientId::try_new("analytics").unwrap())
        .unwrap()
        .audience_mapper = None;
    provider.seed_realm(realm, state);
    hand_edit_in_git(&plane, |text| add_extra_callback(&text)).await;

    sweep(&plane.router).await;

    assert_eq!(
        analytics_in_provider(&provider).audience_mapper,
        None,
        "a withheld client is not written at all, not even to correct it"
    );
}

#[tokio::test]
async fn restoring_the_projection_lets_the_next_sweep_converge_it() {
    let (plane, provider) = converged_newco().await;
    let edited = hand_edit_in_git(&plane, |text| add_extra_callback(&text)).await;
    sweep(&plane.router).await;

    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    let mut restored =
        json!({"realm": identity["realm"], "roles": identity["roles"], "clients": identity["clients"]});
    restored["clients"][0]["redirect"]["uris"] = json!(["https://newco.example.com/callback", CALLBACK]);
    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/newco/identity")
            .header(header::IF_MATCH, format!("\"{edited}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(restored.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    settled(&plane, &entity_tag(&response)).await;
    sweep(&plane.router).await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(identity["reconciliation"]["status"], "applied", "{identity}");
    assert_eq!(identity["reconciliation"]["withheld"], json!([]));
    assert!(!analytics_in_provider(&provider)
        .redirect_uris
        .iter()
        .any(|uri| uri.as_str() == EXTRA));
}

#[tokio::test]
async fn a_reassertion_onto_a_diverged_entry_is_refused_writes_nothing_and_is_not_audited() {
    let (plane, provider) = converged_newco().await;
    let realm = RealmName::try_new("newco").unwrap();
    let mut state = provider.realm(&realm).unwrap();
    state
        .clients
        .get_mut(&OidcClientId::try_new("analytics").unwrap())
        .unwrap()
        .enabled = false;
    provider.seed_realm(realm, state);
    let revision = hand_edit_in_git(&plane, |text| add_extra_callback(&text)).await;
    let before = analytics_in_provider(&provider);

    let response = reassert(&plane.router, &revision).await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = support::json(response).await;
    assert_eq!(body["error"]["code"], "projected_client_diverged", "{body}");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("analytics") && message.contains("redirectUris"),
        "{message}"
    );
    assert!(!message.contains("https://"), "no callback is echoed: {message}");
    assert_eq!(
        analytics_in_provider(&provider),
        before,
        "the Keycloak hand edit is not overwritten by a Git edit"
    );
    assert_eq!(reasserted_audits(&plane).await, 0);
}

#[tokio::test]
async fn an_unverifiable_projection_is_reported_withheld_and_refuses_reassertion() {
    let (plane, provider) = converged_newco().await;
    let before = analytics_in_provider(&provider);
    let revision = hand_edit_in_git(&plane, |text| {
        add_extra_callback(&text).replace("    region: NZ\n", "    region: [NZ]\n")
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "unverifiable", "unverified": ["analytics"]})
    );

    sweep(&plane.router).await;
    assert_eq!(analytics_in_provider(&provider), before);
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(identity["reconciliation"]["status"], "withheld", "{identity}");

    let response = reassert(&plane.router, &revision).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        support::json(response).await["error"]["code"],
        "desired_state_invalid"
    );
    assert_eq!(reasserted_audits(&plane).await, 0);
}

/// Removes `newco`'s declared identity clients from the stored text.
fn without_identity_clients(text: &str) -> String {
    let start = text.find("    clients:\n").unwrap();
    let end = text.find("  product:\n").unwrap();
    format!("{}    clients: []\n{}", &text[..start], &text[end..])
}

#[tokio::test]
async fn reordered_callbacks_are_consistent_and_converge() {
    let (plane, _provider) = converged_newco().await;
    hand_edit_in_git(&plane, |text| {
        text.replace(
            &format!("- https://newco.example.com/callback\n        - {CALLBACK}"),
            &format!("- {CALLBACK}\n        - https://newco.example.com/callback"),
        )
    })
    .await;
    let stored = plane.repository.get(&newco()).await.unwrap();
    assert_eq!(
        stored.document.client().identity.clients[0].redirect.uris()[0].as_str(),
        CALLBACK,
        "the edit must really have reordered the callbacks"
    );

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
    sweep(&plane.router).await;
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(identity["reconciliation"]["status"], "applied", "{identity}");
}

#[tokio::test]
async fn an_unverifiable_projection_with_nothing_to_withhold_is_still_not_ready() {
    let (plane, _provider) = converged_newco().await;
    hand_edit_in_git(&plane, |text| {
        without_identity_clients(&text).replace("    region: NZ\n", "    region: [NZ]\n")
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "unverifiable", "unverified": []})
    );
    sweep(&plane.router).await;
    let client = get(&plane.router, "/api/clients/newco").await;
    let identity_unmet = client["lifecycle"]["unmet"]
        .as_array()
        .unwrap()
        .iter()
        .find(|unmet| unmet["condition"] == "identity")
        .cloned()
        .unwrap_or_else(|| panic!("an unverifiable projection is never ready: {client}"));
    assert!(identity_unmet["detail"]
        .as_str()
        .unwrap()
        .contains("cannot be checked against their projection"));
}

#[tokio::test]
async fn a_reassertion_onto_a_diverged_entry_keeps_the_existing_refusal_order_and_touches_nothing() {
    let (plane, provider) = converged_newco().await;
    let revision = hand_edit_in_git(&plane, |text| add_extra_callback(&text)).await;
    let status_before = plane.statuses.report(&newco());
    provider.clear_calls();

    let anonymous = send(
        &plane.router,
        http::Request::builder()
            .method("POST")
            .uri("/api/clients/newco/identity/reassertion")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let unconditional = send(
        &plane.router,
        as_operator("POST", "/api/clients/newco/identity/reassertion")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unconditional.status(), StatusCode::PRECONDITION_REQUIRED);
    let stale = reassert(&plane.router, "stale").await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(support::json(stale).await["error"]["code"], "revision_conflict");
    let diverged = reassert(&plane.router, &revision).await;
    assert_eq!(diverged.status(), StatusCode::CONFLICT);
    assert_eq!(
        support::json(diverged).await["error"]["code"],
        "projected_client_diverged"
    );

    assert!(
        provider.calls().is_empty(),
        "no provider observation or write: {:?}",
        provider.calls()
    );
    assert_eq!(plane.statuses.report(&newco()), status_before, "no status write");
    assert_eq!(
        plane.repository.get(&newco()).await.unwrap().revision.as_str(),
        revision,
        "no desired-state write"
    );
    assert_eq!(reasserted_audits(&plane).await, 0);
}
