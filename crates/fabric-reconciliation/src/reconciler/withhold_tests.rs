//! A pass that withholds declared clients (D01-19b): they are neither
//! created, written nor corrected, and everything else still converges.

use std::sync::Arc;

use fabric_client_model::{Client, OidcClient, OidcClientId};

use crate::fixtures::{acme, web_client};
use crate::status::ReconciliationStatus;
use crate::testing::FakeIdentityProvider;
use crate::IdentityReconciler;

fn reconciler() -> (IdentityReconciler, Arc<FakeIdentityProvider>) {
    let provider = Arc::new(FakeIdentityProvider::new());

    (IdentityReconciler::new(provider.clone()), provider)
}

fn portal() -> OidcClient {
    OidcClient {
        id: OidcClientId::try_new("portal").unwrap(),
        ..web_client()
    }
}

/// `acme` declaring `web` and `portal`.
fn acme_with_portal() -> Client {
    let mut client = acme();
    client.identity.clients.push(portal());
    client
}

/// Removes `id`'s audience mapper in the provider: product-owned drift an
/// ordinary pass corrects.
fn remove_mapper(provider: &FakeIdentityProvider, id: &OidcClientId) {
    let realm = acme().identity.realm;
    let mut current = provider.realm(&realm).unwrap();
    current.clients.get_mut(id).unwrap().audience_mapper = None;
    provider.seed_realm(realm, current);
}

#[tokio::test]
async fn a_withheld_client_is_not_created_but_the_realm_and_the_rest_are() {
    let (reconciler, provider) = reconciler();

    let outcome = reconciler.reconcile(&acme_with_portal(), &[portal().id]).await;

    assert_eq!(outcome.withheld(), [portal().id]);
    let realm = provider.realm(&acme().identity.realm).unwrap();
    assert!(realm.clients.contains_key(&web_client().id));
    assert!(!realm.clients.contains_key(&portal().id));
}

#[tokio::test]
async fn a_withheld_client_is_not_corrected_while_its_neighbour_is() {
    let (reconciler, provider) = reconciler();
    reconciler.reconcile(&acme_with_portal(), &[]).await;
    remove_mapper(&provider, &web_client().id);
    remove_mapper(&provider, &portal().id);

    provider.clear_calls();
    let outcome = reconciler.reconcile(&acme_with_portal(), &[portal().id]).await;

    assert_eq!(outcome.status(), ReconciliationStatus::Applied);
    assert_eq!(outcome.withheld(), [portal().id]);
    assert_eq!(
        provider.calls(),
        [
            "observe_realm:acme".to_owned(),
            "update_oidc_client:acme:web".to_owned()
        ],
        "only the client that was not withheld is written"
    );
    let realm = provider.realm(&acme().identity.realm).unwrap();
    assert!(realm.clients[&portal().id].audience_mapper.is_none());
}

#[tokio::test]
async fn repeated_passes_that_withhold_a_client_never_write() {
    let (reconciler, provider) = reconciler();
    reconciler.reconcile(&acme_with_portal(), &[]).await;
    remove_mapper(&provider, &portal().id);

    for _ in 0..3 {
        provider.clear_calls();
        let outcome = reconciler.reconcile(&acme_with_portal(), &[portal().id]).await;

        assert!(outcome.changed_nothing());
        assert_eq!(provider.calls(), ["observe_realm:acme"]);
    }
}

#[tokio::test]
async fn withholding_a_client_that_is_not_declared_withholds_nothing() {
    let (reconciler, _) = reconciler();

    let outcome = reconciler.reconcile(&acme(), &[portal().id]).await;

    assert!(outcome.withheld().is_empty());
    assert_eq!(outcome.status(), ReconciliationStatus::Applied);
}
