//! What the verifier refuses, and what it refuses to believe.
//!
//! Signed with a symmetric key rather than an embedded RSA private key — the
//! choice the control plane's OIDC tests already made. What is under test is
//! the decision sequence, not the arithmetic of RSA.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use fabric_core::Clock;
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
use serde_json::json;

use crate::{
    IssuerRegistration, KeyCache, KeySet, KeySource, RefusalReason, Registry, VerificationError, Verifier,
};

const SECRET: &[u8] = b"a-test-signing-secret-not-used-anywhere-real";
const OTHER_SECRET: &[u8] = b"a-different-secret-belonging-to-somebody-else";
const KID: &str = "test-key-1";
const ISSUER: &str = "https://identity.example/realms/acme";
/// Real wall-clock seconds.
///
/// The library validates `exp` and `nbf` against the system clock, not against
/// any clock this crate injects — the injected one measures key staleness and
/// nothing else. A fixture pinned to a constant would be permanently expired.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the system clock is after the epoch")
        .as_secs()
}

/// A clock that does not move, so validity windows are exact.
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        now()
    }
}

/// A key source that always publishes the test key.
struct AlwaysPublishes;

#[async_trait]
impl KeySource for AlwaysPublishes {
    async fn fetch(&self, _jwks_uri: &str) -> Result<KeySet, String> {
        Ok(KeySet::from_entries([(
            KID.to_owned(),
            DecodingKey::from_secret(SECRET),
        )]))
    }
}

fn registration() -> IssuerRegistration {
    IssuerRegistration {
        tenant: "acme".to_owned(),
        issuer: ISSUER.to_owned(),
        audience: "workspec".to_owned(),
        jwks_uri: "https://keycloak.internal/certs".to_owned(),
        algorithms: vec![Algorithm::HS256],
        store: "01ACMESTORE".to_owned(),
        max_key_age_seconds: 43_200,
    }
}

fn verifier() -> Verifier {
    let registry = Registry::build([registration()]).expect("a valid registry");
    let cache = Arc::new(KeyCache::new(Arc::new(AlwaysPublishes), Arc::new(FixedClock)));

    Verifier::new(registry, cache)
}

/// A token, with every part overridable so one test breaks one thing.
fn token_with(claims: &serde_json::Value, algorithm: Algorithm, kid: Option<&str>, secret: &[u8]) -> String {
    let mut header = Header::new(algorithm);
    header.kid = kid.map(std::borrow::ToOwned::to_owned);

    encode(&header, claims, &EncodingKey::from_secret(secret)).expect("the fixture should encode")
}

/// The ordinary, entirely valid token.
fn good_claims() -> serde_json::Value {
    json!({
        "iss": ISSUER,
        "aud": "workspec",
        "sub": "cb606ddc-f148-4193-8875-a84ea6a85e6c",
        "exp": now() + 300,
        "nbf": now() - 10,
    })
}

fn refused(error: &VerificationError) -> RefusalReason {
    match error {
        VerificationError::Refused(reason) => *reason,
        VerificationError::Unavailable(reason) => {
            panic!("expected a refusal, got unavailable: {reason}")
        }
    }
}

#[tokio::test]
async fn a_valid_token_yields_an_identity_bound_to_the_registry() {
    let token = token_with(&good_claims(), Algorithm::HS256, Some(KID), SECRET);

    let identity = verifier().verify(&token).await.expect("a valid token verifies");

    assert_eq!(identity.tenant(), "acme");
    assert_eq!(identity.subject(), "cb606ddc-f148-4193-8875-a84ea6a85e6c");
    assert_eq!(identity.store(), "01ACMESTORE");
    assert_eq!(
        identity.principal().to_string(),
        "acme/cb606ddc-f148-4193-8875-a84ea6a85e6c"
    );
}

#[tokio::test]
async fn claims_naming_a_tenant_a_store_or_a_principal_are_ignored_entirely() {
    // The injection case. A token may say whatever it likes about which tenant
    // it belongs to; the answer comes from the registration its verified
    // issuer selected, and nothing else.
    let mut claims = good_claims();
    claims["tenant"] = json!("someone-else");
    claims["realm"] = json!("someone-else");
    claims["store"] = json!("01VICTIMSTORE");
    claims["store_id"] = json!("01VICTIMSTORE");
    claims["principal"] = json!("someone-else/root");

    let token = token_with(&claims, Algorithm::HS256, Some(KID), SECRET);
    let identity = verifier()
        .verify(&token)
        .await
        .expect("the token is otherwise valid");

    assert_eq!(identity.tenant(), "acme", "the tenant came from the registry");
    assert_eq!(
        identity.store(),
        "01ACMESTORE",
        "the store came from the registry"
    );
    assert_eq!(
        identity.principal().realm(),
        "acme",
        "the realm came from the registry, not from a claim"
    );
}

#[tokio::test]
async fn an_issuer_the_registry_does_not_name_is_refused() {
    let mut claims = good_claims();
    claims["iss"] = json!("https://identity.example/realms/rogue");

    let token = token_with(&claims, Algorithm::HS256, Some(KID), SECRET);

    assert_eq!(
        refused(&verifier().verify(&token).await.expect_err("unknown issuer")),
        RefusalReason::UnknownIssuer
    );
}

#[tokio::test]
async fn a_token_signed_by_somebody_else_is_refused() {
    let token = token_with(&good_claims(), Algorithm::HS256, Some(KID), OTHER_SECRET);

    assert_eq!(
        refused(&verifier().verify(&token).await.expect_err("wrong key")),
        RefusalReason::BadSignature
    );
}

#[tokio::test]
async fn the_wrong_audience_is_refused() {
    let mut claims = good_claims();
    claims["aud"] = json!("some-other-service");

    let token = token_with(&claims, Algorithm::HS256, Some(KID), SECRET);

    assert_eq!(
        refused(&verifier().verify(&token).await.expect_err("wrong audience")),
        RefusalReason::WrongAudience
    );
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let mut claims = good_claims();
    claims["exp"] = json!(now() - 300);

    let token = token_with(&claims, Algorithm::HS256, Some(KID), SECRET);

    assert_eq!(
        refused(&verifier().verify(&token).await.expect_err("expired")),
        RefusalReason::OutsideValidity
    );
}

#[tokio::test]
async fn a_token_that_is_not_yet_valid_is_refused() {
    let mut claims = good_claims();
    claims["nbf"] = json!(now() + 600);

    let token = token_with(&claims, Algorithm::HS256, Some(KID), SECRET);

    assert_eq!(
        refused(&verifier().verify(&token).await.expect_err("not yet valid")),
        RefusalReason::OutsideValidity
    );
}

#[tokio::test]
async fn the_tolerance_for_clock_skew_is_bounded_and_deliberate() {
    // The library's default is 60 seconds. A token 45 seconds expired must be
    // refused, which is only true because the tolerance was set rather than
    // inherited.
    let mut claims = good_claims();
    claims["exp"] = json!(now() - 45);

    let token = token_with(&claims, Algorithm::HS256, Some(KID), SECRET);

    assert_eq!(
        refused(&verifier().verify(&token).await.expect_err("beyond tolerance")),
        RefusalReason::OutsideValidity
    );
}

#[tokio::test]
async fn an_algorithm_the_registration_does_not_permit_is_refused() {
    // Algorithm substitution. The token is perfectly signed — with an
    // algorithm nobody agreed to accept for this issuer.
    let token = token_with(&good_claims(), Algorithm::HS512, Some(KID), SECRET);

    assert_eq!(
        refused(
            &verifier()
                .verify(&token)
                .await
                .expect_err("substituted algorithm")
        ),
        RefusalReason::DisallowedAlgorithm
    );
}

#[tokio::test]
async fn a_token_with_no_key_id_is_refused() {
    // Without a `kid` there is no key to select, and "try them all" is how an
    // implementation ends up accepting a retired key.
    let token = token_with(&good_claims(), Algorithm::HS256, None, SECRET);

    assert_eq!(
        refused(&verifier().verify(&token).await.expect_err("no kid")),
        RefusalReason::Malformed
    );
}

#[tokio::test]
async fn a_token_with_no_subject_is_refused() {
    let mut claims = good_claims();
    claims.as_object_mut().expect("an object").remove("sub");

    let token = token_with(&claims, Algorithm::HS256, Some(KID), SECRET);
    let error = verifier().verify(&token).await.expect_err("no subject");

    // Either the library's required-claims check or ours catches it; what
    // matters is that it is a refusal and never an identity.
    assert!(matches!(
        refused(&error),
        RefusalReason::NoSubject | RefusalReason::BadSignature
    ));
}

#[tokio::test]
async fn a_subject_that_cannot_form_a_principal_is_refused() {
    // A subject carrying the platform's own separator would otherwise mint a
    // principal claiming a different realm.
    let mut claims = good_claims();
    claims["sub"] = json!("evil/root");

    let token = token_with(&claims, Algorithm::HS256, Some(KID), SECRET);

    assert_eq!(
        refused(&verifier().verify(&token).await.expect_err("unusable subject")),
        RefusalReason::UnusableSubject
    );
}

#[tokio::test]
async fn garbage_is_refused_without_reaching_the_registry() {
    for garbage in ["", "not-a-token", "a.b", "....", "a.!!!.c"] {
        let error = verifier().verify(garbage).await.expect_err("garbage");

        assert!(
            matches!(
                refused(&error),
                RefusalReason::Malformed | RefusalReason::NoIssuer
            ),
            "{garbage:?} must be refused as malformed"
        );
    }
}
