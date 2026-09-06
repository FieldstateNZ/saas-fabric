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
            other_protocol_mappers: 0,
            unmodellable_redirect_uris: 0,
            enabled: true,
            standard_flow_enabled: true,
            post_logout_redirect_uris_is_every_registered_uri: true,
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

/// The positive control for `matches()`'s nine terms: enumerates each one
/// against the fixture the tests below mutate, so a future change to any of
/// them cannot silently start from a baseline that was already drifting.
#[test]
fn a_converged_client_is_left_alone() {
    let realm = converged_realm();
    let existing = realm
        .clients
        .get(&web_client().id)
        .expect("the fixture declares this client");

    assert!(existing.public);
    assert_eq!(existing.unmodellable_redirect_uris, 0);
    assert_eq!(existing.challenge_method, Some(PkceMethod::S256));
    assert_eq!(existing.audience_mapper.as_deref(), Some(AUDIENCE));
    assert_eq!(existing.other_protocol_mappers, 0);
    assert!(existing.enabled);
    assert!(existing.standard_flow_enabled);
    assert!(existing.post_logout_redirect_uris_is_every_registered_uri);
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

/// D13a. A declared URI is present in the provider's records but under a
/// changed value — parseable, so this already worked before ADR 0019.
/// Distinguished from D13 (a URI the model cannot parse at all) and D13b (an
/// extra, undeclared URI the provider also holds).
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
            other_protocol_mappers: 0,
            unmodellable_redirect_uris: 0,
            enabled: true,
            standard_flow_enabled: true,
            post_logout_redirect_uris_is_every_registered_uri: true,
        },
    );

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// D13b. Every declared URI is present and parseable, but the provider also
/// holds one more that is not declared. `matches` compares the two sets for
/// equality: a redirect allow-list is a security boundary, and a legitimate
/// extra entry the provider holds beyond what is declared is drift, exactly
/// as a missing one would be.
#[test]
fn an_extra_redirect_uri_the_provider_holds_is_drift() {
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
/// silence (ADR 0019 §6) — the declared set is fully present *and* the
/// provider holds one more entry that does not parse, and that is still a
/// client to rewrite.
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

/// C5 and C6. The provider may hold no PKCE attribute at all — never set, or
/// removed by hand (C5) — or a value this model does not recognise, such as
/// `plain`, empty, or a typo (C6). `challenge_method` reads `None` either
/// way: there is no `Plain` variant anywhere in this model, and none is
/// needed for either case to be seen as drift.
#[test]
fn a_client_without_a_recognised_challenge_method_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.challenge_method = None;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// E15. ADR 0019's deliberate break: a `v1` document migrates to `pkce: S256`
/// when it is read, but a client written to the provider before this ADR has
/// no PKCE attribute at all. The first sweep after deployment corrects it —
/// every public client not already performing PKCE stops working, and this
/// test is what makes that visible rather than a surprise. Same observed
/// shape as [`a_client_without_a_recognised_challenge_method_is_corrected`],
/// not a second code path that could drift out of step with it.
#[test]
fn a_v1_client_is_still_reconciled_with_the_s256_challenge_method() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.challenge_method = None;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// C6a. The audience mapper was removed by hand. ADR 0019 §6: "A client whose
/// mapper was removed by hand stops matching and is rewritten." Without this
/// term the mapper would be written once and could silently disappear,
/// taking the edge's `aud` check down with it.
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

/// A client-level mapper nobody declared, added out of band, is the same
/// drift the audience-mapper terms already catch and gets the same
/// correction: `declaration()` writes a client's whole mapper set and the
/// provider's `PUT` replaces it, so the extra mapper does not survive the
/// next sweep.
#[test]
fn a_client_carrying_a_mapper_nobody_declared_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.other_protocol_mappers = 1;
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

/// A client disabled by hand — through the provider's own console, not this
/// platform — is unreachable to every caller, silently: `enabled` was written
/// once and never observed until this term existed, so every other field
/// still read as converged while nobody could authenticate through it.
#[test]
fn a_client_disabled_by_hand_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.enabled = false;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// A client whose standard (authorization-code) flow was switched off by
/// hand has silently stopped being able to authenticate anyone through it —
/// the same written-but-unobserved gap `enabled` had.
#[test]
fn a_client_whose_standard_flow_was_switched_off_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.standard_flow_enabled = false;
    }

    let plan = plan(&acme(), Some(&realm), AUDIENCE);

    assert_eq!(plan.actions(), [IdentityAction::UpdateOidcClient(web_client())]);
}

/// A post-logout redirect set narrowed by hand is an operator narrowing where
/// a user can land after logging out — drift the same way a redirect URI
/// itself would be, and just as unobserved before this term existed.
#[test]
fn a_client_whose_post_logout_set_was_narrowed_is_corrected() {
    let mut realm = converged_realm();
    if let Some(existing) = realm.clients.get_mut(&web_client().id) {
        existing.post_logout_redirect_uris_is_every_registered_uri = false;
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
            other_protocol_mappers: 0,
            unmodellable_redirect_uris: 0,
            enabled: false,
            standard_flow_enabled: false,
            post_logout_redirect_uris_is_every_registered_uri: false,
        },
    );

    assert!(plan(&acme(), Some(&realm), AUDIENCE).is_converged());
}
