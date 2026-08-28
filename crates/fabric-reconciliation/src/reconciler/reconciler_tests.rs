//! Tests for reconciliation against a provider that behaves like a real one.

use std::sync::Arc;

use crate::fixtures::{acme, role};
use crate::status::ReconciliationStatus;
use crate::testing::FakeIdentityProvider;
use crate::{IdentityReconciler, ProviderError};

/// A reconciler over a fake provider, with the provider kept for assertions.
fn reconciler() -> (IdentityReconciler, Arc<FakeIdentityProvider>) {
    let provider = Arc::new(FakeIdentityProvider::new());

    (IdentityReconciler::new(provider.clone()), provider)
}

#[tokio::test]
async fn a_missing_realm_is_created_with_its_roles_and_clients() {
    let (reconciler, provider) = reconciler();

    let outcome = reconciler.reconcile(&acme()).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Applied);
    assert_eq!(outcome.actions(), 4);

    let realm = provider.realm(&acme().identity.realm).unwrap();
    assert_eq!(realm.display_name, "Acme");
    assert!(realm.roles.contains(&role("Client Realm Administrator")));
    assert!(realm.roles.contains(&role("Client Realm User")));
    assert_eq!(realm.clients.len(), 1);
}

#[tokio::test]
async fn reconciling_again_changes_nothing() {
    let (reconciler, provider) = reconciler();
    reconciler.reconcile(&acme()).await;
    let after_first = provider.realm(&acme().identity.realm);

    provider.clear_calls();
    let outcome = reconciler.reconcile(&acme()).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Applied);
    assert!(outcome.changed_nothing());
    assert_eq!(
        provider.calls(),
        ["observe_realm:acme"],
        "a converged client must cost one read and no writes"
    );
    assert_eq!(provider.realm(&acme().identity.realm), after_first);
}

#[tokio::test]
async fn reconciling_a_third_time_still_changes_nothing() {
    // Idempotency is a property of *repetition*, not of the second call. A
    // reconciler that alternated between two states would pass the test above.
    let (reconciler, provider) = reconciler();

    for _ in 0..3 {
        reconciler.reconcile(&acme()).await;
    }

    provider.clear_calls();
    reconciler.reconcile(&acme()).await;

    assert_eq!(provider.calls(), ["observe_realm:acme"]);
}

#[tokio::test]
async fn only_the_missing_role_is_created() {
    let (reconciler, provider) = reconciler();
    reconciler.reconcile(&acme()).await;

    // Something removed one role out of band.
    let realm = acme().identity.realm;
    let mut current = provider.realm(&realm).unwrap();
    current.roles.remove(&role("Client Realm User"));
    provider.seed_realm(realm.clone(), current);

    provider.clear_calls();
    let outcome = reconciler.reconcile(&acme()).await;

    assert_eq!(outcome.actions(), 1);
    assert_eq!(
        provider.calls(),
        [
            "observe_realm:acme".to_owned(),
            "create_realm_role:acme:Client Realm User".to_owned()
        ]
    );
}

#[tokio::test]
async fn a_provider_that_cannot_be_read_fails_the_pass_and_writes_nothing() {
    let (reconciler, provider) = reconciler();
    provider.fail_with(ProviderError::Unavailable {
        detail: "connection refused".to_owned(),
    });

    let outcome = reconciler.reconcile(&acme()).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Failed);
    assert!(outcome
        .detail()
        .is_some_and(|detail| detail.contains("unavailable")));
    assert_eq!(provider.realm(&acme().identity.realm), None);
}

#[tokio::test]
async fn a_provider_that_fails_mid_apply_leaves_the_pass_failed() {
    let (reconciler, provider) = reconciler();
    provider.fail_with(ProviderError::NotPermitted);

    let outcome = reconciler.reconcile(&acme()).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Failed);
    assert_eq!(outcome.actions(), 0, "a failed pass claims no changes");
}

#[tokio::test]
async fn a_failure_is_recovered_from_on_the_next_pass() {
    // A failed reconciliation must not be terminal: nothing about it corrupts
    // desired state, so the next pass starts from the same document.
    let (reconciler, provider) = reconciler();
    provider.fail_with(ProviderError::Unavailable {
        detail: "connection refused".to_owned(),
    });
    reconciler.reconcile(&acme()).await;

    provider.recover();
    let outcome = reconciler.reconcile(&acme()).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Applied);
    assert!(provider.realm(&acme().identity.realm).is_some());
}

#[tokio::test]
async fn planning_reports_the_read_failure_rather_than_an_empty_plan() {
    // An empty plan means "already converged". Returning one here would report
    // a client as healthy precisely when nothing is known about it.
    let (reconciler, provider) = reconciler();
    provider.fail_with(ProviderError::NotPermitted);

    assert_eq!(reconciler.plan(&acme()).await, Err(ProviderError::NotPermitted));
}
