//! D01-15: a converged client moving between hosts.
//!
//! With trusted provenance of the last successful projection — here the
//! in-memory test ledger, scoped to each pass's revision — the callbacks a
//! move declares are written and the old ones removed, with no false
//! `handEdited`; a hand removal and a disabled client are still held.
//! Without it — no ledger, which is every deployment until a protected
//! continuity store exists, or a restart that lost an in-memory one — the
//! move is held whole before any old callback is removed, until an audited
//! re-assertion.
//!
//! Every case asserts the provider calls the pass made: an ordinary pass
//! may observe, and may write `analytics` only when the move is placeable.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use fabric_client_model::{OidcClientId, RealmName};
use fabric_control_plane::{AuditStream, AuditTrail as _, ClientRepository as _};
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::{ObservedOidcClient, WriteStage};
use http::StatusCode;
use serde_json::json;
use support::newco::{
    analytics_in_provider, converged_on, identity_unmet, internal_callback, newco, provider_callbacks,
    reassert, reconciliation, save_product_on, settled, sweep, INTERNAL_HOST, PUBLIC_CALLBACKS,
};
use support::projected::{analytics_definition, catalogue_commands, newco_on_hosts};
use support::{
    control_plane_with_identity_provider, control_plane_with_identity_provider_and_no_ledger,
    TestControlPlane,
};

/// What every provider read starts with; the fixture's `acme` is observed
/// by every sweep too.
const OBSERVE: &str = "observe_realm:";
const WRITE: &str = "update_oidc_client:newco:analytics";

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// Saves `newco` onto `hosts`, waits for the convergence the save starts,
/// and runs one more sweep. Returns the provider calls all of that made.
async fn moved(
    plane: &TestControlPlane,
    provider: &FakeIdentityProvider,
    hosts: &[&str],
    scheme: Option<&str>,
) -> Vec<String> {
    provider.clear_calls();
    let revision = save_product_on(&plane.router, hosts, scheme).await;
    settled(plane, &revision).await;
    sweep(&plane.router).await;
    provider.calls()
}

/// Calls that observe, and write `analytics` exactly once.
fn observes_and_writes_once(calls: &[String]) {
    assert_eq!(
        calls.iter().filter(|call| call.as_str() == WRITE).count(),
        1,
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|call| call.starts_with(OBSERVE) || call == WRITE),
        "no other provider call: {calls:?}"
    );
}

/// Calls that only observe.
fn observes_only(calls: &[String]) {
    assert!(!calls.is_empty());
    assert!(
        calls.iter().all(|call| call.starts_with(OBSERVE)),
        "no provider write: {calls:?}"
    );
}

fn edit_analytics(provider: &FakeIdentityProvider, edit: impl FnOnce(&mut ObservedOidcClient)) {
    let realm = RealmName::try_new("newco").unwrap();
    let mut state = provider.realm(&realm).unwrap();
    edit(
        state
            .clients
            .get_mut(&OidcClientId::try_new("analytics").unwrap())
            .unwrap(),
    );
    provider.seed_realm(realm, state);
}

#[tokio::test]
async fn a_converged_public_to_internal_save_writes_the_private_callback_and_removes_the_public_ones() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    assert_eq!(provider_callbacks(&provider), set(&PUBLIC_CALLBACKS));

    let calls = moved(&plane, &provider, &[INTERNAL_HOST], Some("http")).await;

    observes_and_writes_once(&calls);
    assert_eq!(provider_callbacks(&provider), set(&[&internal_callback("http")]));
    let reconciliation = reconciliation(&plane.router).await;
    assert_eq!(reconciliation["status"], "applied", "{reconciliation}");
    assert_eq!(reconciliation["handEdits"], json!([]), "no false handEdited");
    assert!(!identity_unmet(&plane.router).await);
}

#[tokio::test]
async fn a_converged_internal_to_public_save_writes_the_public_callbacks_and_removes_the_private_one() {
    let (plane, provider) = converged_on(&[INTERNAL_HOST], Some("https")).await;

    let calls = moved(&plane, &provider, &["newco.example.com"], None).await;

    observes_and_writes_once(&calls);
    assert_eq!(provider_callbacks(&provider), set(&PUBLIC_CALLBACKS));
    assert_eq!(reconciliation(&plane.router).await["handEdits"], json!([]));
}

#[tokio::test]
async fn a_converged_public_to_public_save_replaces_the_host_callback_and_keeps_the_template() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;

    let calls = moved(&plane, &provider, &["portal.example.com"], None).await;

    observes_and_writes_once(&calls);
    assert_eq!(
        provider_callbacks(&provider),
        set(&[
            "https://newco.example.com/callback",
            "https://portal.example.com/analytics/callback"
        ])
    );
    let reconciliation = reconciliation(&plane.router).await;
    assert_eq!(reconciliation["status"], "applied", "{reconciliation}");
    assert_eq!(reconciliation["handEdits"], json!([]));
}

#[tokio::test]
async fn a_scheme_change_on_internal_hosts_is_written_too() {
    let (plane, provider) = converged_on(&[INTERNAL_HOST], Some("http")).await;

    let calls = moved(&plane, &provider, &[INTERNAL_HOST], Some("https")).await;

    observes_and_writes_once(&calls);
    assert_eq!(provider_callbacks(&provider), set(&[&internal_callback("https")]));
}

#[tokio::test]
async fn each_write_is_recorded_by_digest_before_and_after_bound_to_its_revision() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    let created = support::newco::current_revision(&plane.router).await;
    moved(&plane, &provider, &[INTERNAL_HOST], Some("http")).await;
    let saved = support::newco::current_revision(&plane.router).await;

    let records: Vec<_> = plane
        .ledger
        .as_ref()
        .unwrap()
        .records()
        .into_iter()
        .filter(|record| record.client == newco())
        .collect();
    let summary: Vec<_> = records
        .iter()
        .map(|record| {
            (
                record.stage,
                record.revision.as_ref().unwrap().as_str().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (WriteStage::Writing, created.clone()),
            (WriteStage::Written, created),
            (WriteStage::Writing, saved.clone()),
            (WriteStage::Written, saved),
        ],
        "creation and the move, each bound to the revision it converged, and nothing for the passes between"
    );
    assert!(records.iter().all(|record| record.realm.as_str() == "newco"));
    assert!(records
        .iter()
        .all(|record| record.callbacks.iter().all(|digest| digest.as_str().len() == 64)));
}

#[tokio::test]
async fn a_callback_removed_by_hand_after_a_move_is_still_held() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    moved(&plane, &provider, &[INTERNAL_HOST], Some("http")).await;
    edit_analytics(&provider, |client| client.redirect_uris.clear());
    provider.clear_calls();

    sweep(&plane.router).await;

    observes_only(&provider.calls());
    assert!(provider_callbacks(&provider).is_empty(), "never written back");
    let reconciliation = reconciliation(&plane.router).await;
    assert_eq!(reconciliation["status"], "handEdited");
    assert_eq!(
        reconciliation["handEdits"],
        json!([{"client": "analytics", "field": "declaredRedirectUriRemoved"}])
    );
    assert!(identity_unmet(&plane.router).await);
}

#[tokio::test]
async fn a_callback_removed_by_hand_beside_a_move_stays_removed_while_the_new_one_is_written() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    edit_analytics(&provider, |client| {
        client
            .redirect_uris
            .retain(|uri| uri.as_str() != "https://newco.example.com/callback");
    });

    let calls = moved(&plane, &provider, &["portal.example.com"], None).await;

    observes_and_writes_once(&calls);
    assert_eq!(
        provider_callbacks(&provider),
        set(&["https://portal.example.com/analytics/callback"]),
        "the hand-removed template callback is not restored"
    );
    assert_eq!(
        reconciliation(&plane.router).await["handEdits"],
        json!([{"client": "analytics", "field": "declaredRedirectUriRemoved"}])
    );
}

#[tokio::test]
async fn a_client_disabled_by_hand_moves_and_stays_disabled() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    edit_analytics(&provider, |client| client.enabled = false);

    let calls = moved(&plane, &provider, &[INTERNAL_HOST], Some("http")).await;

    observes_and_writes_once(&calls);
    assert!(
        !analytics_in_provider(&provider).enabled,
        "containment is not undone by a move"
    );
    assert_eq!(provider_callbacks(&provider), set(&[&internal_callback("http")]));
    assert_eq!(
        reconciliation(&plane.router).await["handEdits"],
        json!([{"client": "analytics", "field": "clientDisabled"}])
    );
}

/// Asserts `plane` holds the move to `.internal` whole: no provider write, the
/// public callbacks untouched, reported, identity unmet.
async fn held_whole(plane: &TestControlPlane, provider: &FakeIdentityProvider) {
    let calls = moved(plane, provider, &[INTERNAL_HOST], Some("http")).await;

    observes_only(&calls);
    assert_eq!(
        provider_callbacks(provider),
        set(&PUBLIC_CALLBACKS),
        "held before any old callback is removed"
    );
    let held = reconciliation(&plane.router).await;
    assert_eq!(held["status"], "handEdited", "{held}");
    assert_eq!(
        held["handEdits"],
        json!([{"client": "analytics", "field": "redirectUrisUnconfirmed"}])
    );
    assert!(identity_unmet(&plane.router).await);
}

/// A re-assertion writes the held move, and is audited.
async fn reasserted(plane: &TestControlPlane, provider: &FakeIdentityProvider) {
    let response = reassert(&plane.router).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(provider_callbacks(provider), set(&[&internal_callback("http")]));
    let audited = plane.audit.read(&AuditStream::Client(newco())).await.unwrap();
    assert!(audited
        .entries
        .iter()
        .any(|entry| entry.record().event() == "control_plane.audit.identity_projection_reasserted"));
}

#[tokio::test]
async fn with_no_ledger_a_converged_client_is_left_alone_and_a_move_is_held_until_reasserted() {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider_and_no_ledger(provider.clone());
    let revision = newco_on_hosts(&plane.router, &["newco.example.com"], None).await;
    settled(&plane, &revision).await;
    provider.clear_calls();
    sweep(&plane.router).await;
    observes_only(&provider.calls());
    assert_eq!(
        reconciliation(&plane.router).await["status"],
        "applied",
        "nothing is missing"
    );

    held_whole(&plane, &provider).await;
    reasserted(&plane, &provider).await;

    let calls = moved(&plane, &provider, &[INTERNAL_HOST], Some("https")).await;
    observes_only(&calls);
    assert_eq!(
        provider_callbacks(&provider),
        set(&[&internal_callback("http")]),
        "with no ledger every later move is held too"
    );
}

#[tokio::test]
async fn after_a_restart_lost_the_baseline_a_move_is_held_until_reasserted() {
    let (first, provider) = converged_on(&["newco.example.com"], None).await;
    let stored = first.repository.get(&newco()).await.unwrap();
    let plane = control_plane_with_identity_provider(provider.clone());
    catalogue_commands(
        &plane.router,
        serde_json::Value::Null,
        vec![
            json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
            json!({"action":"saveApplication","id":"analytics","definition":analytics_definition()}),
            json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
        ],
    )
    .await;
    plane.repository.insert(&stored.document).unwrap();
    // The continuity store is durable across the restart; the ledger is not.
    plane.enroll("newco").await;

    held_whole(&plane, &provider).await;
    reasserted(&plane, &provider).await;

    let calls = moved(&plane, &provider, &[INTERNAL_HOST], Some("https")).await;
    observes_and_writes_once(&calls);
    assert_eq!(
        provider_callbacks(&provider),
        set(&[&internal_callback("https")]),
        "the re-assertion recorded a baseline, so the next move needs none"
    );
}

#[tokio::test]
async fn a_hand_removal_with_no_baseline_is_held_and_never_written_back() {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider_and_no_ledger(provider.clone());
    let revision = newco_on_hosts(&plane.router, &["newco.example.com"], None).await;
    settled(&plane, &revision).await;
    edit_analytics(&provider, |client| client.redirect_uris.clear());
    provider.clear_calls();

    sweep(&plane.router).await;

    observes_only(&provider.calls());
    assert!(provider_callbacks(&provider).is_empty());
    assert_eq!(
        reconciliation(&plane.router).await["handEdits"],
        json!([{"client": "analytics", "field": "redirectUrisUnconfirmed"}])
    );
}
