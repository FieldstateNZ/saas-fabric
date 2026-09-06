//! Tests for what the diff decides has to change.

use std::collections::{BTreeMap, BTreeSet};

use fabric_client_model::{PkceMethod, RedirectUri};

use crate::fixtures::{acme, role, web_client, ROLES};
use crate::plan::{plan, IdentityAction};
use crate::provider::{ObservedOidcClient, ObservedRealm};

/// The audience a converged fixture client's mapper carries.
///
/// Not read by `matches()` yet — that comparison is slice 5's — so this is
/// only what these fixtures need to hold a plausible, converged shape.
const AUDIENCE: &str = "saas-fabric-data-api";

/// A realm holding exactly what `acme()` declares, plus two roles the provider
/// created for itself.
fn converged_realm() -> ObservedRealm {
    let mut roles: BTreeSet<_> = ROLES.into_iter().map(role).collect();
    roles.insert(role("offline_access"));
    roles.insert(role("uma_authorization"));

    let mut clients = BTreeMap::new();
    clients.insert(
        web_client().id,
        ObservedOidcClient {
            redirect_uris: web_client().redirect.uris().iter().cloned().collect(),
            public: true,
            challenge_method: Some(PkceMethod::S256),
            audience_mapper: Some(AUDIENCE.to_owned()),
            unmodellable_redirect_uris: 0,
        },
    );

    ObservedRealm {
        display_name: "Acme".to_owned(),
        roles,
        clients,
    }
}

#[test]
fn an_absent_realm_is_created_before_anything_is_put_in_it() {
    let plan = plan(&acme(), None);

    assert!(matches!(
        plan.actions().first(),
        Some(IdentityAction::CreateRealm { .. })
    ));
    assert_eq!(
        plan.actions().len(),
        4,
        "realm, two roles, one application client"
    );
}

#[test]
fn a_realm_that_already_matches_produces_no_actions() {
    let plan = plan(&acme(), Some(&converged_realm()));

    assert!(plan.is_converged(), "{:?}", plan.actions());
}

#[test]
fn roles_the_provider_created_for_itself_are_not_a_difference() {
    // The failure this pins: comparing role sets for equality rather than
    // containment would try to "correct" every realm forever.
    let realm = converged_realm();

    assert!(realm.roles.len() > acme().identity.roles.len());
    assert!(plan(&acme(), Some(&realm)).is_converged());
}

#[test]
fn only_the_missing_role_is_created() {
    let mut realm = converged_realm();
    realm.roles.remove(&role("Client Realm User"));

    let plan = plan(&acme(), Some(&realm));

    assert_eq!(
        plan.actions(),
        [IdentityAction::CreateRealmRole(role("Client Realm User"))]
    );
}

#[test]
fn a_changed_display_name_is_corrected() {
    let mut realm = converged_realm();
    realm.display_name = "Acme Limited".to_owned();

    let plan = plan(&acme(), Some(&realm));

    assert_eq!(
        plan.actions(),
        [IdentityAction::SetRealmDisplayName {
            display_name: "Acme".to_owned()
        }]
    );
}

#[test]
fn an_undeclared_role_is_left_alone_rather_than_deleted() {
    let mut realm = converged_realm();
    realm.roles.insert(role("Invoicing Approver"));

    assert!(plan(&acme(), Some(&realm)).is_converged());
}

#[test]
fn an_application_client_with_a_changed_redirect_uri_is_updated() {
    let mut realm = converged_realm();
    realm.clients.insert(
        web_client().id,
        ObservedOidcClient {
            redirect_uris: [RedirectUri::try_new("https://evil.example.com/callback").unwrap()]
                .into_iter()
                .collect(),
            public: true,
            challenge_method: Some(PkceMethod::S256),
            audience_mapper: Some(AUDIENCE.to_owned()),
            unmodellable_redirect_uris: 0,
        },
    );

    let plan = plan(&acme(), Some(&realm));

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

#[test]
fn a_declared_client_switched_to_confidential_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.public = false;
    }

    let plan = plan(&acme(), Some(&realm));

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

#[test]
fn an_undeclared_application_client_is_left_alone() {
    let mut realm = converged_realm();
    realm.clients.insert(
        fabric_client_model::OidcClientId::try_new("legacy").unwrap(),
        ObservedOidcClient {
            redirect_uris: BTreeSet::new(),
            public: true,
            challenge_method: None,
            audience_mapper: None,
            unmodellable_redirect_uris: 0,
        },
    );

    assert!(plan(&acme(), Some(&realm)).is_converged());
}
