//! Tests for what the diff decides has to change.

use std::collections::{BTreeMap, BTreeSet};

use fabric_client_model::{PkceMethod, RedirectUri};

use crate::fixtures::{acme, role, web_client, ROLES};
use crate::plan::{plan, IdentityAction};
use crate::provider::{ObservedOidcClient, ObservedRealm};

/// The audience a converged fixture client's mapper carries, and the value
/// every test in this file passes as `plan`'s `configured_audience`.
///
/// Read by `matches()`: this is the deployment's own audience string (ADR
/// 0019 §1, §G5), threaded into the comparison rather than carried by either
/// fixture, because it is adapter configuration and neither a document nor a
/// realm holds it.
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
    let plan = plan(&acme(), None, AUDIENCE);

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
    let plan = plan(&acme(), Some(&converged_realm()), AUDIENCE);

    assert!(plan.is_converged(), "{:?}", plan.actions());
}

/// The positive control for `matches()`'s five terms: enumerates each one
/// against the fixture the tests below mutate, so a future change to any of
/// them cannot silently start from a baseline that was already drifting.
#[test]
fn a_converged_native_client_is_left_alone() {
    let realm = converged_realm();
    let existing = realm
        .clients
        .get(&web_client().id)
        .expect("the fixture declares this client");

    assert!(existing.public);
    assert_eq!(existing.unmodellable_redirect_uris, 0);
    assert_eq!(existing.challenge_method, Some(PkceMethod::S256));
    assert_eq!(existing.audience_mapper.as_deref(), Some(AUDIENCE));
    assert_eq!(
        existing.redirect_uris,
        web_client()
            .redirect
            .uris()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
    );

    assert!(plan(&acme(), Some(&realm), AUDIENCE).is_converged());
}

#[test]
fn roles_the_provider_created_for_itself_are_not_a_difference() {
    // The failure this pins: comparing role sets for equality rather than
    // containment would try to "correct" every realm forever.
    let realm = converged_realm();

    assert!(realm.roles.len() > acme().identity.roles.len());
    assert!(plan(&acme(), Some(&realm), AUDIENCE).is_converged());
}

#[test]
fn only_the_missing_role_is_created() {
    let mut realm = converged_realm();
    realm.roles.remove(&role("Client Realm User"));

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(
        plan.actions(),
        [IdentityAction::CreateRealmRole(role("Client Realm User"))]
    );
}

#[test]
fn a_changed_display_name_is_corrected() {
    let mut realm = converged_realm();
    realm.display_name = "Acme Limited".to_owned();

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

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

    assert!(plan(&acme(), Some(&realm), AUDIENCE).is_converged());
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

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// D13a. Every declared URI is present *and* parseable, but Keycloak also
/// holds one more that is not declared. `matches` compares the sets for
/// equality, not containment, so a legitimate extra entry is drift too — a
/// redirect allow-list is a security boundary, and "the declared set is a
/// subset of what Keycloak accepts" is not the guarantee this model makes.
#[test]
fn an_extra_redirect_uri_keycloak_holds_is_drift() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing
            .redirect_uris
            .insert(RedirectUri::try_new("https://www.example.com/extra").unwrap());
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// D13. An observed redirect URI this model cannot parse is drift, not
/// silence (ADR 0019 §6) — the declared set is fully present *and* Keycloak
/// holds one more entry that does not parse, and that is still a client to
/// rewrite.
///
/// **Mutation-proved.** With the `existing.unmodellable_redirect_uris == 0`
/// term deleted from `plan::diff::matches`, this test failed:
/// `a_redirect_uri_this_model_cannot_parse_is_drift` reported
/// `plan.actions() == []` where `[UpdateOidcClient(web_client())]` was
/// expected, because every other term still matched. Restored, and recorded
/// in `docs/verification.md`.
#[test]
fn a_redirect_uri_this_model_cannot_parse_is_drift() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.unmodellable_redirect_uris = 1;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// C5. The PKCE attribute is simply absent — created before this ADR, or
/// removed by hand. `challenge_method` reads `None` either way.
#[test]
fn a_client_whose_pkce_attribute_was_removed_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.challenge_method = None;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// C9. Whether Keycloak holds no PKCE attribute or a value this model does
/// not recognise (`plain`, empty, a typo), `challenge_method` reads `None`
/// either way — there is no `Plain` variant to construct here, which is the
/// point: this is the same observed shape as the attribute being absent
/// ([`a_client_whose_pkce_attribute_was_removed_is_corrected`]), not a second
/// code path that could drift out of step with it.
#[test]
fn a_client_with_a_challenge_method_this_model_cannot_read_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.challenge_method = None;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// E15. ADR 0019's deliberate break: a `v1` document migrates to `pkce: S256`
/// when it is read, but a client written to Keycloak before this ADR has no
/// PKCE attribute at all. The first sweep after deployment corrects it —
/// every public client not already performing PKCE stops working, and this
/// test is what makes that visible rather than a surprise.
#[test]
fn a_v1_client_is_still_reconciled_with_the_s256_challenge_method() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.challenge_method = None;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// C6. ADR 0019 §6: "A client whose mapper was removed by hand stops
/// matching and is rewritten." Without this term the mapper would be written
/// once and could silently disappear, taking the edge's `aud` check down
/// with it.
#[test]
fn a_client_whose_audience_mapper_was_removed_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.audience_mapper = None;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// A mapper that is present but names a different audience is exactly as
/// wrong as an absent one: the client's single mapper satisfies neither route
/// it needs to (ADR 0019 §1's equality constraint).
#[test]
fn a_client_whose_audience_mapper_names_another_audience_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.audience_mapper = Some("some-other-audience".to_owned());
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// E2. A declared client is always public — see
/// [`fabric_client_model::OidcClient`] for why a confidential one cannot be
/// expressed — so a client the provider now holds as confidential does not
/// match, and is corrected.
#[test]
fn a_declared_client_switched_to_confidential_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.public = false;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

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

    assert!(plan(&acme(), Some(&realm), AUDIENCE).is_converged());
}
