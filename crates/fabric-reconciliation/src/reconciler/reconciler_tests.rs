//! Tests for reconciliation against a provider that behaves like a real one.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_client_model::{OidcClient, RealmName, RoleName};

use crate::fixtures::{acme, role, web_client};
use crate::status::ReconciliationStatus;
use crate::testing::FakeIdentityProvider;
use crate::{IdentityProvider, IdentityReconciler, ObservedRealm, ProviderError};

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
async fn a_client_whose_mapper_was_removed_is_corrected_on_the_next_pass() {
    // The client-level analogue of `only_the_missing_role_is_created`: proves
    // a correction reaches the provider, not only the plan.
    let (reconciler, provider) = reconciler();
    reconciler.reconcile(&acme()).await;

    // Something removed the client's audience mapper out of band.
    let realm = acme().identity.realm;
    let mut current = provider.realm(&realm).unwrap();
    if let Some(client) = current.clients.get_mut(&web_client().id) {
        client.audience_mapper = None;
    }
    provider.seed_realm(realm.clone(), current);

    provider.clear_calls();
    let outcome = reconciler.reconcile(&acme()).await;

    assert_eq!(outcome.actions(), 1);
    assert_eq!(
        provider.calls(),
        [
            "observe_realm:acme".to_owned(),
            format!("update_oidc_client:acme:{}", web_client().id)
        ]
    );

    let corrected = provider.realm(&realm).unwrap();
    let client = corrected.clients.get(&web_client().id).unwrap();
    assert_eq!(
        client.audience_mapper.as_deref(),
        provider.configured_audience(),
        "the mapper must be rewritten to the provider's own audience, not merely re-planned"
    );
}

/// B4. Uses `with_audience` rather than the zero-arg constructor, so the
/// corrected value asserted below is provably this test's own choice and not
/// merely the fake's default — the client-level analogue of
/// `a_client_whose_mapper_was_removed_is_corrected_on_the_next_pass`, this
/// time for a mapper that is present but stale rather than absent.
#[tokio::test]
async fn a_client_whose_mapper_names_another_audience_is_corrected_by_the_reconciler() {
    let provider = Arc::new(FakeIdentityProvider::with_audience("acme-data-api"));
    let reconciler = IdentityReconciler::new(provider.clone());
    reconciler.reconcile(&acme()).await;

    // Something wrote the mapper for a different deployment's audience.
    let realm = acme().identity.realm;
    let mut current = provider.realm(&realm).unwrap();
    if let Some(client) = current.clients.get_mut(&web_client().id) {
        client.audience_mapper = Some("some-other-deployments-audience".to_owned());
    }
    provider.seed_realm(realm.clone(), current);

    provider.clear_calls();
    let outcome = reconciler.reconcile(&acme()).await;

    assert_eq!(outcome.actions(), 1);
    assert_eq!(
        provider.calls(),
        [
            "observe_realm:acme".to_owned(),
            format!("update_oidc_client:acme:{}", web_client().id)
        ]
    );

    let corrected = provider.realm(&realm).unwrap();
    let client = corrected.clients.get(&web_client().id).unwrap();
    assert_eq!(client.audience_mapper.as_deref(), Some("acme-data-api"));
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

/// N3 (ADR 0019 §1, §G5). A provider that observes fine but names no audience
/// at all must not be planned against: building one would either skip the
/// audience-mapper comparison silently or invent a value nobody configured,
/// and either way could report a client converged when its `aud` has never
/// actually been checked. `plan` refuses instead — fail closed, not "mark
/// every client drifted".
#[tokio::test]
async fn a_provider_that_names_no_audience_refuses_to_plan() {
    struct NoAudience;

    #[async_trait]
    impl IdentityProvider for NoAudience {
        async fn observe_realm(&self, _realm: &RealmName) -> Result<Option<ObservedRealm>, ProviderError> {
            Ok(None)
        }

        async fn create_realm(&self, _realm: &RealmName, _display_name: &str) -> Result<(), ProviderError> {
            unreachable!("plan() must refuse before anything is applied")
        }

        async fn set_realm_display_name(
            &self,
            _realm: &RealmName,
            _display_name: &str,
        ) -> Result<(), ProviderError> {
            unreachable!("plan() must refuse before anything is applied")
        }

        async fn create_realm_role(&self, _realm: &RealmName, _role: &RoleName) -> Result<(), ProviderError> {
            unreachable!("plan() must refuse before anything is applied")
        }

        async fn create_oidc_client(
            &self,
            _realm: &RealmName,
            _client: &OidcClient,
        ) -> Result<(), ProviderError> {
            unreachable!("plan() must refuse before anything is applied")
        }

        async fn update_oidc_client(
            &self,
            _realm: &RealmName,
            _client: &OidcClient,
        ) -> Result<(), ProviderError> {
            unreachable!("plan() must refuse before anything is applied")
        }

        fn configured_audience(&self) -> Option<&str> {
            None
        }

        fn describe(&self) -> String {
            "a provider with no configured audience".to_owned()
        }
    }

    let reconciler = IdentityReconciler::new(Arc::new(NoAudience));

    assert_eq!(
        reconciler.plan(&acme()).await,
        Err(ProviderError::NoAudienceConfigured)
    );

    let outcome = reconciler.reconcile(&acme()).await;
    assert_eq!(outcome.status(), ReconciliationStatus::Failed);
}
