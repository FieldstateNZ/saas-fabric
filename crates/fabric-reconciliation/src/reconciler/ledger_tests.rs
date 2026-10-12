//! D01-15: what a pass records around each client write, and how a host
//! transition, a hand removal and every ledger failure play out against a
//! provider that behaves like a real one.

use std::collections::BTreeSet;
use std::sync::Arc;

use fabric_client_model::{Client, OidcClient, RedirectStrategy, RedirectStrategyKind, RedirectUri};

use crate::fixtures::{acme, web_client};
use crate::status::ReconciliationStatus;
use crate::testing::{FakeIdentityProvider, InMemoryDeclarationLedger};
use crate::{HandEdit, HandEditedField, IdentityReconciler, WriteStage};

const OLD: [&str; 2] = [
    "https://www.example.com/callback",
    "https://www.example.com/silent",
];
const INTERNAL: &str = "http://acme.lucentroot.internal/web/callback";

fn moved_to_internal() -> Client {
    let mut client = acme();
    client.identity.clients = vec![OidcClient {
        redirect: RedirectStrategy::try_new(
            RedirectStrategyKind::PrivateNetwork,
            vec![RedirectUri::try_new(INTERNAL).unwrap()],
        )
        .unwrap(),
        ..web_client()
    }];
    client
}

fn callbacks(provider: &FakeIdentityProvider) -> BTreeSet<String> {
    provider.realm(&acme().identity.realm).unwrap().clients[&web_client().id]
        .redirect_uris
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn stages(ledger: &InMemoryDeclarationLedger) -> Vec<WriteStage> {
    ledger.records().into_iter().map(|record| record.stage).collect()
}

fn unconfirmed() -> Vec<HandEdit> {
    vec![HandEdit {
        client: web_client().id,
        field: HandEditedField::RedirectUrisUnconfirmed,
    }]
}

/// `acme` converged through a reconciler keeping `ledger`.
async fn converged(
    ledger: Option<Arc<InMemoryDeclarationLedger>>,
) -> (IdentityReconciler, Arc<FakeIdentityProvider>) {
    let provider = Arc::new(FakeIdentityProvider::new());
    let reconciler = IdentityReconciler::new(provider.clone());
    let reconciler = match ledger {
        Some(ledger) => reconciler.with_ledger(ledger),
        None => reconciler,
    };
    reconciler.reconcile(&acme(), &[]).await;
    provider.clear_calls();
    (reconciler, provider)
}

#[tokio::test]
async fn a_created_client_is_recorded_on_both_sides_of_the_write_and_only_once() {
    let ledger = Arc::new(InMemoryDeclarationLedger::new());
    let (reconciler, _provider) = converged(Some(ledger.clone())).await;

    assert_eq!(stages(&ledger), [WriteStage::Writing, WriteStage::Written]);
    reconciler.reconcile(&acme(), &[]).await;
    assert_eq!(ledger.records().len(), 2, "a converged pass records nothing");
}

#[tokio::test]
async fn a_host_transition_writes_the_new_callbacks_removes_the_old_and_reports_no_hand_edit() {
    let ledger = Arc::new(InMemoryDeclarationLedger::new());
    let (reconciler, provider) = converged(Some(ledger.clone())).await;

    let outcome = reconciler.reconcile(&moved_to_internal(), &[]).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Applied);
    assert!(outcome.hand_edits().is_empty(), "{outcome:?}");
    assert_eq!(callbacks(&provider), set(&[INTERNAL]));
    assert_eq!(ledger.records().len(), 4);
    assert!(reconciler
        .reconcile(&moved_to_internal(), &[])
        .await
        .changed_nothing());
}

#[tokio::test]
async fn a_hand_removal_after_a_transition_is_still_held() {
    let ledger = Arc::new(InMemoryDeclarationLedger::new());
    let (reconciler, provider) = converged(Some(ledger.clone())).await;
    reconciler.reconcile(&moved_to_internal(), &[]).await;
    let realm = acme().identity.realm;
    let mut state = provider.realm(&realm).unwrap();
    state
        .clients
        .get_mut(&web_client().id)
        .unwrap()
        .redirect_uris
        .clear();
    provider.seed_realm(realm, state);
    provider.clear_calls();

    let outcome = reconciler.reconcile(&moved_to_internal(), &[]).await;

    assert_eq!(
        outcome.hand_edits(),
        [HandEdit {
            client: web_client().id,
            field: HandEditedField::DeclaredRedirectUriRemoved
        }]
    );
    assert_eq!(provider.calls(), ["observe_realm:acme"], "never written back");
}

#[tokio::test]
async fn with_no_ledger_a_transition_is_held_whole_and_a_reassertion_writes_it() {
    let (reconciler, provider) = converged(None).await;

    let outcome = reconciler.reconcile(&moved_to_internal(), &[]).await;

    assert_eq!(outcome.hand_edits(), unconfirmed());
    assert_eq!(
        callbacks(&provider),
        set(&OLD),
        "nothing removed before the transition is confirmed"
    );
    assert_eq!(provider.calls(), ["observe_realm:acme"]);

    let reasserted = reconciler.reassert(&moved_to_internal()).await;
    assert_eq!(reasserted.status(), ReconciliationStatus::Applied);
    assert_eq!(callbacks(&provider), set(&[INTERNAL]));
}

#[tokio::test]
async fn an_unreadable_ledger_holds_the_transition_like_no_ledger() {
    let ledger = Arc::new(InMemoryDeclarationLedger::new());
    let (reconciler, provider) = converged(Some(ledger.clone())).await;
    ledger.set_unreadable(true);

    let outcome = reconciler.reconcile(&moved_to_internal(), &[]).await;

    assert_eq!(outcome.hand_edits(), unconfirmed());
    assert_eq!(callbacks(&provider), set(&OLD));
}

#[tokio::test]
async fn a_writing_record_refused_calls_no_provider_write_and_the_next_pass_completes() {
    let ledger = Arc::new(InMemoryDeclarationLedger::new());
    let (reconciler, provider) = converged(Some(ledger.clone())).await;
    ledger.refuse(Some(WriteStage::Writing));

    let outcome = reconciler.reconcile(&moved_to_internal(), &[]).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Failed);
    assert_eq!(outcome.actions(), 0);
    assert_eq!(
        provider.calls(),
        ["observe_realm:acme"],
        "no write without its record"
    );
    assert_eq!(callbacks(&provider), set(&OLD));

    ledger.refuse(None);
    let retried = reconciler.reconcile(&moved_to_internal(), &[]).await;
    assert_eq!(retried.status(), ReconciliationStatus::Applied);
    assert!(retried.hand_edits().is_empty());
    assert_eq!(callbacks(&provider), set(&[INTERNAL]));
}

#[tokio::test]
async fn a_written_record_refused_leaves_the_write_counted_and_its_callbacks_unplaceable() {
    let ledger = Arc::new(InMemoryDeclarationLedger::new());
    let (reconciler, provider) = converged(Some(ledger.clone())).await;
    ledger.refuse(Some(WriteStage::Written));

    let outcome = reconciler.reconcile(&moved_to_internal(), &[]).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Failed);
    assert_eq!(outcome.actions(), 1, "the provider confirmed the write");
    assert_eq!(callbacks(&provider), set(&[INTERNAL]));

    ledger.refuse(None);
    let realm = acme().identity.realm;
    let mut state = provider.realm(&realm).unwrap();
    state
        .clients
        .get_mut(&web_client().id)
        .unwrap()
        .redirect_uris
        .clear();
    provider.seed_realm(realm, state);
    provider.clear_calls();

    let after = reconciler.reconcile(&moved_to_internal(), &[]).await;
    assert_eq!(
        after.hand_edits(),
        unconfirmed(),
        "never re-written over a possible hand removal, never called one"
    );
    assert_eq!(provider.calls(), ["observe_realm:acme"]);
}
