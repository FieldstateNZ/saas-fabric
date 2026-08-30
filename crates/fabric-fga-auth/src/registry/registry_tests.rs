//! What the registry refuses to be built from.
//!
//! Every case here is fatal at startup, and that is the point: each one
//! describes a verifier that would either authenticate nobody or authenticate
//! somebody by accident, and neither is a state worth discovering from a
//! request.

use jsonwebtoken::Algorithm;

use crate::{ConfigurationError, IssuerRegistration, Registry};

/// A registration that is valid, so a test can break exactly one thing.
fn acme() -> IssuerRegistration {
    IssuerRegistration {
        tenant: "acme".to_owned(),
        issuer: "https://identity.example/realms/acme".to_owned(),
        audience: "workspec".to_owned(),
        jwks_uri: "https://keycloak.identity.svc.cluster.local/realms/acme/certs".to_owned(),
        algorithms: vec![Algorithm::RS256],
        store: "01ABC".to_owned(),
        authorization_model_id: "01ACMEMODEL".to_owned(),
        max_key_age_seconds: 43_200,
    }
}

#[test]
fn an_empty_registry_is_refused() {
    // Not defaulted, not warned about. An empty issuer list is the shape in
    // which at least one real authorization service stops verifying signatures
    // altogether, silently.
    let error = Registry::build([]).expect_err("an empty registry must be refused");

    assert_eq!(error, ConfigurationError::NoIssuers);
}

#[test]
fn a_duplicated_issuer_is_refused() {
    let error = Registry::build([acme(), acme()]).expect_err("a duplicate must be refused");

    assert_eq!(
        error,
        ConfigurationError::DuplicateIssuer {
            issuer: "https://identity.example/realms/acme".to_owned(),
        }
    );
}

#[test]
fn two_tenants_sharing_a_store_is_not_this_modules_problem() {
    // Refusing it here would be inventing a rule. Whether two tenants may
    // share a store is a platform decision, not a registry-parsing one.
    let mut foo = acme();
    foo.tenant = "foo".to_owned();
    foo.issuer = "https://identity.example/realms/foo".to_owned();

    assert!(Registry::build([acme(), foo]).is_ok());
}

#[test]
fn a_tenant_that_is_not_a_realm_identity_is_refused() {
    // The tenant becomes the realm half of every principal minted here, so a
    // tenant that cannot be one is a registration that can never mint an
    // identity — caught at startup rather than on the first sign-in.
    let mut broken = acme();
    broken.tenant = "Acme Corp".to_owned();

    assert!(matches!(
        Registry::build([broken]),
        Err(ConfigurationError::InvalidRegistration { .. })
    ));
}

#[test]
fn every_field_a_registration_cannot_do_without_is_checked() {
    type Break = fn(&mut IssuerRegistration);

    let cases: [(&str, Break); 7] = [
        ("empty issuer", |r| r.issuer = String::new()),
        ("empty audience", |r| r.audience = String::new()),
        ("empty jwks_uri", |r| r.jwks_uri = String::new()),
        ("no algorithms", |r| r.algorithms = Vec::new()),
        ("empty store", |r| r.store = String::new()),
        ("zero max key age", |r| r.max_key_age_seconds = 0),
        // Without a pinned model the service uses its most recent one, so
        // writing a model would deploy it. Refused at startup rather than
        // discovered when a decision quietly changes.
        ("no pinned model", |r| r.authorization_model_id = String::new()),
    ];

    for (described, break_it) in cases {
        let mut registration = acme();
        break_it(&mut registration);

        assert!(
            matches!(
                Registry::build([registration]),
                Err(ConfigurationError::InvalidRegistration { .. })
            ),
            "{described} must be refused at startup"
        );
    }
}

#[test]
fn an_issuer_is_matched_exactly_and_never_by_prefix() {
    let registry = Registry::build([acme()]).expect("valid");

    assert!(registry
        .registration("https://identity.example/realms/acme")
        .is_some());
    // A prefix match here would let anybody who controls a longer path look
    // like this issuer.
    assert!(registry
        .registration("https://identity.example/realms/acme/evil")
        .is_none());
    assert!(registry
        .registration("https://identity.example/realms/ac")
        .is_none());
    assert!(registry
        .registration("https://identity.example/realms/acmex")
        .is_none());
}
