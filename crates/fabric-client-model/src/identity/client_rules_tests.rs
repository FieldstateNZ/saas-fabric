//! Tests for the entitlement one application client is validated against.
//!
//! Every row here is a security boundary. The document states which kind of
//! callback a client is entitled to, and a URI outside that kind is refused —
//! never reclassified into a strategy that would take it.

use crate::identity::required_roles::REQUIRED_ROLES;
use crate::{AppScheme, ClientProtocol, DesiredStateError, IdentityConfiguration, OidcClient};
use crate::{OidcClientId, PkceMethod, RealmName, RedirectStrategy, RedirectStrategyKind, RedirectUri};

/// A configuration holding one client with the given strategy and callbacks.
///
/// Built through `try_new` where the strategy is legal and assembled through
/// the document otherwise, so a refusal is proved where an operator meets it.
fn identity_with(kind: &RedirectStrategyKind, uris: &[&str]) -> IdentityConfiguration {
    let redirect = strategy(kind, uris);

    IdentityConfiguration {
        realm: RealmName::try_new("acme").unwrap(),
        roles: REQUIRED_ROLES
            .into_iter()
            .map(|name| crate::RoleName::try_new(name).unwrap())
            .collect(),
        clients: vec![OidcClient {
            id: OidcClientId::try_new("web").unwrap(),
            protocol: ClientProtocol::Oidc,
            pkce: PkceMethod::S256,
            redirect,
        }],
    }
}

/// A strategy assembled through the document, so a rule-breaking pairing can
/// be built for the test that proves it is refused.
fn strategy(kind: &RedirectStrategyKind, uris: &[&str]) -> RedirectStrategy {
    let mut listed = String::new();
    for uri in uris {
        listed.push_str("  - ");
        listed.push_str(uri);
        listed.push('\n');
    }

    let block = if listed.is_empty() {
        "uris: []\n".to_owned()
    } else {
        format!("uris:\n{listed}")
    };
    let text = format!("strategy: {kind}\n{}{block}", scheme_line(kind));

    serde_norway::from_str(&text).unwrap_or_else(|error| panic!("{text} must deserialise: {error}"))
}

/// The `scheme` key, for the one variant that carries it.
fn scheme_line(kind: &RedirectStrategyKind) -> String {
    match kind {
        RedirectStrategyKind::CustomScheme(scheme) => format!("scheme: {scheme}\n"),
        _ => String::new(),
    }
}

/// The private-use scheme the deferred variant is exercised with.
fn slipway() -> RedirectStrategyKind {
    RedirectStrategyKind::CustomScheme(AppScheme::try_new("nz.fieldstate.slipway").unwrap())
}

/// The `InvalidField` detail this pairing was refused with, if it was.
fn refusal_detail(kind: &RedirectStrategyKind, uris: &[&str]) -> Option<String> {
    match identity_with(kind, uris).validate() {
        Err(DesiredStateError::InvalidField {
            field: "spec.identity.clients",
            detail,
        }) => Some(detail),
        _ => None,
    }
}

/// Whether validating this pairing produced an `InvalidField` on the clients.
fn refuses_client(kind: &RedirectStrategyKind, uris: &[&str]) -> bool {
    refusal_detail(kind, uris).is_some()
}

/// Asserts a refusal's detail names the strategy, the URI's kind, and what
/// the strategy admits — matrix D1's expectation for every row this helper
/// covers.
///
/// The third assertion is against `kind.admitted()` itself rather than the
/// word "admits". An operator is not helped by being told a strategy admits
/// *something*; the point of the row is that the message says what.
fn assert_names_strategy_kind_and_admission(kind: &RedirectStrategyKind, uris: &[&str], uri_kind: &str) {
    let detail = refusal_detail(kind, uris).unwrap_or_else(|| panic!("{kind} must refuse {uris:?}"));

    assert!(detail.contains(&kind.to_string()), "{detail}");
    assert!(detail.contains(uri_kind), "{detail}");
    assert!(detail.contains(kind.admitted()), "{detail}");
}

/// Whether validating this pairing was accepted.
fn accepts(kind: &RedirectStrategyKind, uris: &[&str]) -> bool {
    identity_with(kind, uris).validate().is_ok()
}

#[test]
fn a_development_callback_is_refused_under_the_production_strategy() {
    // D1: the detail names the strategy, the URI's kind, and what the
    // strategy admits, so an operator does not have to guess which of the
    // three is wrong.
    assert_names_strategy_kind_and_admission(
        &RedirectStrategyKind::ClaimedHttps,
        &["http://localhost:5173/callback"],
        "loopback callback",
    );
}

#[test]
fn a_loopback_host_is_not_a_claimed_https_callback_even_over_tls() {
    // D1a. The host rule's sharpest edge. "The scheme is https, therefore the
    // strategy is claimedHttps" is the intuition the partition exists to break.
    assert_names_strategy_kind_and_admission(
        &RedirectStrategyKind::ClaimedHttps,
        &["https://localhost:5173/callback"],
        "loopback callback",
    );
}

#[test]
fn a_private_network_host_is_not_a_claimed_https_callback_even_over_tls() {
    // D1b.
    assert_names_strategy_kind_and_admission(
        &RedirectStrategyKind::ClaimedHttps,
        &["https://admin.corp.internal/cb"],
        "private-network callback",
    );
}

#[test]
fn an_upper_case_loopback_host_is_still_a_loopback_host() {
    assert!(refuses_client(
        &RedirectStrategyKind::ClaimedHttps,
        &["https://LOCALHOST:5173/cb"]
    ));
}

#[test]
fn an_upper_case_internal_host_is_still_a_private_network_host() {
    assert!(refuses_client(
        &RedirectStrategyKind::ClaimedHttps,
        &["https://ADMIN.CORP.INTERNAL/cb"]
    ));
}

#[test]
fn a_loopback_callback_may_be_served_over_tls() {
    // A developer running a local TLS proxy writes exactly this. It is
    // loopback, so `development` is the strategy that holds it.
    assert!(accepts(
        &RedirectStrategyKind::Development,
        &["https://localhost/cb"]
    ));
}

#[test]
fn a_public_callback_is_refused_under_the_development_strategy() {
    // D2. Refused rather than waved through as "stricter than needed": the
    // strategy is a statement about what the client *is*.
    assert_names_strategy_kind_and_admission(
        &RedirectStrategyKind::Development,
        &["https://www.example.com/callback"],
        "public https callback",
    );
}

#[test]
fn a_private_network_callback_is_refused_under_the_production_strategy() {
    // D3.
    assert_names_strategy_kind_and_admission(
        &RedirectStrategyKind::ClaimedHttps,
        &["http://acme.lucentroot.internal/callback"],
        "private-network callback",
    );
}

#[test]
fn a_private_use_scheme_is_not_a_loopback_redirect() {
    assert!(refuses_client(
        &RedirectStrategyKind::Development,
        &["nz.fieldstate.slipway:/cb"]
    ));
}

#[test]
fn a_private_use_scheme_with_a_loopback_authority_is_still_a_private_use_scheme() {
    assert!(refuses_client(
        &RedirectStrategyKind::Development,
        &["nz.fieldstate.slipway://localhost/cb"]
    ));
}

#[test]
fn a_wildcard_callback_is_refused_under_the_production_strategy() {
    let refusal = identity_with(
        &RedirectStrategyKind::ClaimedHttps,
        &["https://www.example.com/*"],
    )
    .validate()
    .unwrap_err();

    assert!(refusal.to_string().contains("RFC 9700"), "{refusal}");
}

#[test]
fn a_wildcard_callback_is_refused_under_the_private_network_strategy() {
    assert!(refuses_client(
        &RedirectStrategyKind::PrivateNetwork,
        &["http://acme.lucentroot.internal/*"]
    ));
}

#[test]
fn a_trailing_path_wildcard_is_the_one_place_a_development_callback_may_use_one() {
    assert!(accepts(
        &RedirectStrategyKind::Development,
        &["http://localhost:5173/*"]
    ));
}

#[test]
fn a_loopback_callback_with_no_port_admits_any_port() {
    // RFC 8252 §7.3: a native application binds an ephemeral port, so the
    // authorization server has to allow whichever one it got. Observed on
    // Keycloak 26.0.8, 2026-09-06: over `http` a loopback URI registered
    // without a port matches the same path on any port. Over `https`, and
    // whenever a port is written, the match is exact — which this model
    // cannot enforce and does not claim to.
    assert!(accepts(
        &RedirectStrategyKind::Development,
        &["http://127.0.0.1/callback"]
    ));
}

#[test]
fn the_custom_scheme_strategy_is_refused_with_the_phase_that_will_carry_it() {
    let refusal = identity_with(&slipway(), &["nz.fieldstate.slipway:/cb"])
        .validate()
        .unwrap_err();

    assert!(matches!(refusal, DesiredStateError::Deferred { .. }), "{refusal}");
    assert!(refusal.to_string().contains("Lane E phase 2"), "{refusal}");
}

#[test]
fn a_strategy_with_no_callback_could_never_sign_a_user_in() {
    // Reconciliation would create the client, Keycloak would accept it, and
    // the first login attempt would fail in a different system weeks later.
    assert!(refuses_client(&RedirectStrategyKind::ClaimedHttps, &[]));
}

#[test]
fn a_callback_on_a_scheme_the_strategy_did_not_declare_is_refused() {
    // Registering one application's callback against another's is the
    // interception RFC 8252 §8.6 warns about. Proved through the constructor,
    // because validation refuses every custom-scheme client as deferred first
    // and would report that whether the pairing was right or wrong.
    let other = RedirectStrategyKind::CustomScheme(AppScheme::try_new("com.example.other").unwrap());
    let uri = RedirectUri::try_new("nz.fieldstate.slipway:/cb").unwrap();

    assert!(RedirectStrategy::try_new(other, vec![uri]).is_err());
    assert!(RedirectStrategy::try_new(
        slipway(),
        vec![RedirectUri::try_new("nz.fieldstate.slipway:/cb").unwrap()]
    )
    .is_ok());
}

#[test]
fn every_admitted_pairing_is_accepted() {
    assert!(accepts(
        &RedirectStrategyKind::ClaimedHttps,
        &["https://www.example.com/callback"]
    ));
    assert!(accepts(
        &RedirectStrategyKind::PrivateNetwork,
        &["http://acme.lucentroot.internal/cb", "https://x.internal/cb"]
    ));
    assert!(accepts(
        &RedirectStrategyKind::Development,
        &["http://[::1]:5173/cb", "http://localhost/cb"]
    ));
}
