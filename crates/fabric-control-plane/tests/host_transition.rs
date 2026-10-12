//! D01-15: a converged client moving between hosts. The callbacks the move
//! declares are written and the old ones removed, with no false `handEdited`,
//! because each write is recorded in the declaration ledger — the durable
//! audit trail — and a missing callback is classified against what Fabric
//! wrote. A hand removal and a disabled client are still held. A client
//! whose baseline is unknown is held whole, before any callback is removed,
//! until an operator re-asserts.

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
use http::StatusCode;
use serde_json::json;
use support::newco::{
    converged_on, identity_unmet, internal_callback, newco, provider_callbacks, reassert, reconciliation,
    save_product_on, settled, sweep, INTERNAL_HOST, PUBLIC_CALLBACKS,
};
use support::projected::{analytics_definition, catalogue_commands};
use support::{control_plane_with_identity_provider, TestControlPlane};

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// Saves `newco` onto `hosts`, then waits for the convergence the save
/// starts and runs one more sweep.
async fn moved(plane: &TestControlPlane, hosts: &[&str], scheme: Option<&str>) {
    let revision = save_product_on(&plane.router, hosts, scheme).await;
    settled(plane, &revision).await;
    sweep(&plane.router).await;
}

async fn declaration_records(plane: &TestControlPlane) -> Vec<(String, String)> {
    plane
        .audit
        .read(&AuditStream::Client(newco()))
        .await
        .unwrap()
        .entries
        .iter()
        .map(fabric_control_plane::AuditEntry::record)
        .filter(|record| record.event().contains("identity_declaration"))
        .map(|record| {
            (
                record.event().to_owned(),
                record.field("callbacks").unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn edit_analytics(
    provider: &FakeIdentityProvider,
    edit: impl FnOnce(&mut fabric_reconciliation::ObservedOidcClient),
) {
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

    moved(&plane, &[INTERNAL_HOST], Some("http")).await;

    assert_eq!(provider_callbacks(&provider), set(&[&internal_callback("http")]));
    let reconciliation = reconciliation(&plane.router).await;
    assert_eq!(reconciliation["status"], "applied", "{reconciliation}");
    assert_eq!(reconciliation["handEdits"], json!([]), "no false handEdited");
    assert!(!identity_unmet(&plane.router).await);
}

#[tokio::test]
async fn a_converged_public_to_public_save_replaces_the_host_callback_and_keeps_the_template() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;

    moved(&plane, &["portal.example.com"], None).await;

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
async fn an_internal_to_public_save_and_a_scheme_change_are_written_too() {
    let (plane, provider) = converged_on(&[INTERNAL_HOST], Some("http")).await;

    moved(&plane, &[INTERNAL_HOST], Some("https")).await;
    assert_eq!(provider_callbacks(&provider), set(&[&internal_callback("https")]));

    moved(&plane, &["newco.example.com"], None).await;
    assert_eq!(provider_callbacks(&provider), set(&PUBLIC_CALLBACKS));
    assert_eq!(reconciliation(&plane.router).await["handEdits"], json!([]));
}

#[tokio::test]
async fn each_write_is_recorded_by_digest_before_and_after_and_names_no_callback() {
    let (plane, _provider) = converged_on(&["newco.example.com"], None).await;
    moved(&plane, &[INTERNAL_HOST], Some("http")).await;

    let records = declaration_records(&plane).await;
    let events: Vec<&str> = records.iter().map(|(event, _)| event.as_str()).collect();
    assert_eq!(
        events,
        [
            "control_plane.audit.identity_declaration_writing",
            "control_plane.audit.identity_declaration_written",
            "control_plane.audit.identity_declaration_writing",
            "control_plane.audit.identity_declaration_written",
        ],
        "creation and the move, and nothing for the passes in between"
    );
    for (_, callbacks) in &records {
        assert!(!callbacks.contains("://"), "{callbacks}");
        assert!(
            callbacks.split(',').all(|digest| digest.len() == 64),
            "{callbacks}"
        );
    }
    plane
        .audit
        .read(&AuditStream::Client(newco()))
        .await
        .unwrap()
        .verify()
        .expect("the stream still verifies");
}

#[tokio::test]
async fn a_callback_removed_by_hand_after_a_move_is_still_held() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    moved(&plane, &[INTERNAL_HOST], Some("http")).await;
    edit_analytics(&provider, |client| client.redirect_uris.clear());

    sweep(&plane.router).await;

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
async fn a_client_disabled_by_hand_moves_and_stays_disabled() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    edit_analytics(&provider, |client| client.enabled = false);

    moved(&plane, &[INTERNAL_HOST], Some("http")).await;

    let analytics = support::newco::analytics_in_provider(&provider);
    assert!(!analytics.enabled, "containment is not undone by a move");
    assert_eq!(provider_callbacks(&provider), set(&[&internal_callback("http")]));
    assert_eq!(
        reconciliation(&plane.router).await["handEdits"],
        json!([{"client": "analytics", "field": "clientDisabled"}])
    );
}

/// `newco` as converged by `first`, now read by a control plane whose audit
/// trail has never seen it: a fresh or lost store, so no baseline.
async fn without_a_baseline(
    first: &TestControlPlane,
    provider: Arc<FakeIdentityProvider>,
) -> TestControlPlane {
    let stored = first.repository.get(&newco()).await.unwrap();
    let plane = control_plane_with_identity_provider(provider);
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
    plane
}

#[tokio::test]
async fn with_no_baseline_a_move_is_held_whole_and_an_audited_reassertion_establishes_one() {
    let (first, provider) = converged_on(&["newco.example.com"], None).await;
    let plane = without_a_baseline(&first, provider.clone()).await;
    sweep(&plane.router).await;
    assert_eq!(
        reconciliation(&plane.router).await["status"],
        "applied",
        "a converged client is not held"
    );
    provider.clear_calls();

    moved(&plane, &[INTERNAL_HOST], Some("http")).await;

    assert_eq!(
        provider_callbacks(&provider),
        set(&PUBLIC_CALLBACKS),
        "held before any old callback is removed"
    );
    assert!(
        provider
            .calls()
            .iter()
            .all(|call| call.starts_with("observe_realm")),
        "no provider write: {:?}",
        provider.calls()
    );
    let held = reconciliation(&plane.router).await;
    assert_eq!(held["status"], "handEdited", "{held}");
    assert_eq!(
        held["handEdits"],
        json!([{"client": "analytics", "field": "redirectUrisUnconfirmed"}])
    );
    assert!(identity_unmet(&plane.router).await);

    let response = reassert(&plane.router).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(provider_callbacks(&provider), set(&[&internal_callback("http")]));
    let audited = plane.audit.read(&AuditStream::Client(newco())).await.unwrap();
    assert!(audited
        .entries
        .iter()
        .any(|entry| entry.record().event() == "control_plane.audit.identity_projection_reasserted"));

    moved(&plane, &[INTERNAL_HOST], Some("https")).await;
    assert_eq!(
        provider_callbacks(&provider),
        set(&[&internal_callback("https")]),
        "the re-assertion recorded a baseline, so the next move needs none"
    );
    assert_eq!(reconciliation(&plane.router).await["handEdits"], json!([]));
}
