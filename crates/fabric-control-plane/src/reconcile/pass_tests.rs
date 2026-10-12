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
use crate::testing::InMemoryLifecycleContinuity;
use crate::IntegrationHealth;

fn client() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

/// The `acme` fixture attested with an identity instance, and a
/// continuity store that accepts it exactly as stored.
fn attested_acme() -> (
    Arc<crate::InMemoryClientRepository>,
    fabric_client_model::ClientRevision,
    InMemoryLifecycleContinuity,
) {
    let (repository, _) = repository_with_acme();
    let lifecycle = fabric_client_model::LifecycleRecord {
        instance: Some(fabric_client_model::lifecycle::IdentityInstance::try_new(&"1".repeat(32)).unwrap()),
        ..fabric_client_model::LifecycleRecord::default()
    };
    let document = crate::fixtures::acme_document()
        .with_lifecycle(&lifecycle)
        .unwrap();
    let revision = repository.insert(&document).unwrap();
    let store = InMemoryLifecycleContinuity::new();
    store.enroll(
        &client(),
        lifecycle.instance.clone().unwrap(),
        crate::LifecycleFingerprint::of_document(&document),
    );
    (repository, revision, store)
}

#[tokio::test]
async fn a_sweep_reconciles_every_client_and_records_the_result() {
    let (repository, revision, store) = attested_acme();
    let provider = Arc::new(FakeIdentityProvider::new());
    let reconciler = IdentityReconciler::new(provider.clone());
    let statuses = ReconciliationStatusStore::new();

    let swept = pass::run(
        repository.as_ref(),
        Some(&store),
        &reconciler,
        None,
        &statuses,
        &IntegrationHealth::new(),
        &FixedClock,
    )
    .await;

    assert_eq!(swept, 1);

    let report = statuses.report(&client()).unwrap();
    assert_eq!(report.status, ReconciliationStatus::Applied);
    assert_eq!(report.revision, revision);
    assert_eq!(report.observed_at_unix, FixedClock::UNIX_SECONDS);
    assert!(provider.realm(&RealmName::try_new("acme").unwrap()).is_some());
}

#[tokio::test]
async fn a_second_sweep_changes_nothing() {
    let (repository, _, store) = attested_acme();
    let provider = Arc::new(FakeIdentityProvider::new());
    let reconciler = IdentityReconciler::new(provider.clone());
    let statuses = ReconciliationStatusStore::new();

    pass::run(
        repository.as_ref(),
        Some(&store),
        &reconciler,
        None,
        &statuses,
        &IntegrationHealth::new(),
        &FixedClock,
    )
    .await;
    provider.clear_calls();
    pass::run(
        repository.as_ref(),
        Some(&store),
        &reconciler,
        None,
        &statuses,
        &IntegrationHealth::new(),
        &FixedClock,
    )
    .await;

    assert_eq!(provider.calls(), ["observe_realm:acme"]);
    assert_eq!(
        statuses.report(&client()).unwrap().status,
        ReconciliationStatus::Applied
    );
}

#[tokio::test]
async fn a_provider_failure_is_recorded_and_desired_state_is_untouched() {
    let (repository, revision, store) = attested_acme();
    let provider = Arc::new(FakeIdentityProvider::new());
    provider.fail_with(ProviderError::Unavailable {
        detail: "connection refused".to_owned(),
    });
    let reconciler = IdentityReconciler::new(provider);
    let statuses = ReconciliationStatusStore::new();

    pass::run(
        repository.as_ref(),
        Some(&store),
        &reconciler,
        None,
        &statuses,
        &IntegrationHealth::new(),
        &FixedClock,
    )
    .await;

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
    let (repository, _, store) = attested_acme();
    let provider = Arc::new(FakeIdentityProvider::new());
    let reconciler = IdentityReconciler::new(provider);
    let statuses = ReconciliationStatusStore::new();

    pass::run(
        repository.as_ref(),
        Some(&store),
        &reconciler,
        None,
        &statuses,
        &IntegrationHealth::new(),
        &FixedClock,
    )
    .await;
    let before = statuses.report(&client());

    repository.set_unavailable(Some("connection refused".to_owned()));
    let swept = pass::run(
        repository.as_ref(),
        Some(&store),
        &reconciler,
        None,
        &statuses,
        &IntegrationHealth::new(),
        &FixedClock,
    )
    .await;

    assert_eq!(swept, 0);
    assert_eq!(statuses.report(&client()), before);
}

#[tokio::test]
async fn a_client_the_continuity_store_does_not_confirm_is_left_alone() {
    let tombstoned = fabric_client_model::LifecycleRecord::default()
        .tombstone(1, "o", "retired")
        .unwrap();
    let mismatched = InMemoryLifecycleContinuity::new();
    let instance = fabric_client_model::lifecycle::IdentityInstance::try_new(&"1".repeat(32)).unwrap();
    let realm = RealmName::try_new("acme").unwrap();
    mismatched.enroll(
        &client(),
        instance,
        crate::LifecycleFingerprint::of(&client(), &realm, &tombstoned),
    );

    for continuity in [None, Some(&InMemoryLifecycleContinuity::new()), Some(&mismatched)] {
        let (repository, _) = repository_with_acme();
        let provider = Arc::new(FakeIdentityProvider::new());
        let reconciler = IdentityReconciler::new(provider.clone());
        let statuses = ReconciliationStatusStore::new();

        let swept = pass::run(
            repository.as_ref(),
            continuity.map(|store| store as &dyn crate::LifecycleContinuity),
            &reconciler,
            None,
            &statuses,
            &IntegrationHealth::new(),
            &FixedClock,
        )
        .await;

        assert_eq!(swept, 0);
        assert!(statuses.report(&client()).is_none(), "no status recorded");
        assert!(
            provider.realm(&RealmName::try_new("acme").unwrap()).is_none(),
            "no provider write"
        );
    }
}
