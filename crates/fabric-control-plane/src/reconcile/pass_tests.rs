//! Tests for one sweep over every client.

use std::sync::Arc;

use fabric_client_model::{ClientId, RealmName};
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::{
    IdentityReconciler, ProviderError, ReconciliationStatus, ReconciliationStatusStore,
};

use crate::fixtures::{repository_with_acme, FixedClock};
use crate::reconcile::pass;
use crate::repository::ClientRepository;

fn client() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

#[tokio::test]
async fn a_sweep_reconciles_every_client_and_records_the_result() {
    let (repository, revision) = repository_with_acme();
    let provider = Arc::new(FakeIdentityProvider::new());
    let reconciler = IdentityReconciler::new(provider.clone());
    let statuses = ReconciliationStatusStore::new();

    let swept = pass::run(repository.as_ref(), &reconciler, &statuses, &FixedClock).await;

    assert_eq!(swept, 1);

    let report = statuses.report(&client()).unwrap();
    assert_eq!(report.status, ReconciliationStatus::Applied);
    assert_eq!(report.revision, revision);
    assert_eq!(report.observed_at_unix, FixedClock::UNIX_SECONDS);
    assert!(provider.realm(&RealmName::try_new("acme").unwrap()).is_some());
}

#[tokio::test]
async fn a_second_sweep_changes_nothing() {
    let (repository, _) = repository_with_acme();
    let provider = Arc::new(FakeIdentityProvider::new());
    let reconciler = IdentityReconciler::new(provider.clone());
    let statuses = ReconciliationStatusStore::new();

    pass::run(repository.as_ref(), &reconciler, &statuses, &FixedClock).await;
    provider.clear_calls();
    pass::run(repository.as_ref(), &reconciler, &statuses, &FixedClock).await;

    assert_eq!(provider.calls(), ["observe_realm:acme"]);
    assert_eq!(
        statuses.report(&client()).unwrap().status,
        ReconciliationStatus::Applied
    );
}

#[tokio::test]
async fn a_provider_failure_is_recorded_and_desired_state_is_untouched() {
    let (repository, revision) = repository_with_acme();
    let provider = Arc::new(FakeIdentityProvider::new());
    provider.fail_with(ProviderError::Unavailable {
        detail: "connection refused".to_owned(),
    });
    let reconciler = IdentityReconciler::new(provider);
    let statuses = ReconciliationStatusStore::new();

    pass::run(repository.as_ref(), &reconciler, &statuses, &FixedClock).await;

    assert_eq!(
        statuses.report(&client()).unwrap().status,
        ReconciliationStatus::Failed
    );
    assert_eq!(repository.get(&client()).await.unwrap().revision, revision);
}

#[tokio::test]
async fn an_unreadable_repository_leaves_recorded_status_alone() {
    // A briefly unreadable repository is not evidence that anything changed,
    // so it must not overwrite what is known about every client.
    let (repository, _) = repository_with_acme();
    let provider = Arc::new(FakeIdentityProvider::new());
    let reconciler = IdentityReconciler::new(provider);
    let statuses = ReconciliationStatusStore::new();

    pass::run(repository.as_ref(), &reconciler, &statuses, &FixedClock).await;
    let before = statuses.report(&client());

    repository.set_unavailable(Some("connection refused".to_owned()));
    let swept = pass::run(repository.as_ref(), &reconciler, &statuses, &FixedClock).await;

    assert_eq!(swept, 0);
    assert_eq!(statuses.report(&client()), before);
}
