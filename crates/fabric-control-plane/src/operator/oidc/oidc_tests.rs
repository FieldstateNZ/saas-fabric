//! What the OIDC posture accepts, and — mostly — what it refuses.
//!
//! Tokens here are signed with HS256 against a fixture secret so that no
//! private key has to exist in this repository. The algorithm is the only
//! difference from production; every decision under test — the issuer, the
//! client, the role, the key id, expiry — is made by the same code either way.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use http::{HeaderMap, HeaderValue};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
use serde_json::json;

use super::*;
use crate::operator::OperatorAuthError;

const SECRET: &[u8] = b"fixture-secret-for-operator-token-tests";
const ISSUER: &str = "https://auth.example.test/realms/master";
const CLIENT: &str = "saas-fabric-console";
const ROLE: &str = "fabric-operator";

fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_secs(),
    )
    .expect("the epoch seconds fit in an i64 until the year 292277026596")
}

/// A holder carrying the fixture key, published under `kid`.
fn holder(key_id: Option<&str>) -> Arc<KeyHolder> {
    let holder = KeyHolder::empty();
    holder.replace(VerificationKeys::held(vec![(
        key_id,
        DecodingKey::from_secret(SECRET),
    )]));
    holder
}

fn authenticator(keys: Arc<KeyHolder>) -> OidcOperators {
    OidcOperators::new(ISSUER, CLIENT, ROLE, keys, 60)
        .expect("the fixture posture is valid")
        .signed_symmetrically_for_tests()
}

/// A token, with every field an individual test might want to spoil.
struct Token {
    issuer: String,
    azp: String,
    roles: Vec<String>,
    expires_in: i64,
    key_id: Option<String>,
}

impl Default for Token {
    fn default() -> Self {
        Self {
            issuer: ISSUER.to_owned(),
            azp: CLIENT.to_owned(),
            roles: vec![ROLE.to_owned()],
            expires_in: 300,
            key_id: None,
        }
    }
}

impl Token {
    fn encoded(&self) -> String {
        let mut header = Header::new(Algorithm::HS256);
        header.kid.clone_from(&self.key_id);

        let claims = json!({
            "sub": "f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
            "preferred_username": "brett",
            "iss": self.issuer,
            "azp": self.azp,
            "exp": now() + self.expires_in,
            "iat": now(),
            "realm_access": { "roles": self.roles },
        });

        encode(&header, &claims, &EncodingKey::from_secret(SECRET)).expect("the fixture token should encode")
    }
}

fn presenting(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).expect("a token is a legal header value"),
    );
    headers
}

#[test]
fn accepts_a_token_from_the_realm_naming_the_console_and_carrying_the_role() {
    let operator = authenticator(holder(None))
        .authenticate(&presenting(&Token::default().encoded()))
        .expect("a well-formed operator token should be accepted");

    assert_eq!(operator.subject(), "brett");
}

#[test]
fn refuses_a_request_carrying_no_token_as_missing_rather_than_forbidden() {
    let error = authenticator(holder(None))
        .authenticate(&HeaderMap::new())
        .unwrap_err();

    assert_eq!(error, OperatorAuthError::Missing);
}

#[test]
fn refuses_a_token_from_a_different_issuer() {
    let token = Token {
        issuer: "https://auth.example.test/realms/acme".to_owned(),
        ..Token::default()
    };

    assert_eq!(
        authenticator(holder(None)).authenticate(&presenting(&token.encoded())),
        Err(OperatorAuthError::NotAnOperator)
    );
}

#[test]
fn refuses_a_token_issued_to_another_client_in_the_same_realm() {
    let token = Token {
        azp: "some-other-client".to_owned(),
        ..Token::default()
    };

    assert_eq!(
        authenticator(holder(None)).authenticate(&presenting(&token.encoded())),
        Err(OperatorAuthError::NotAnOperator)
    );
}

#[test]
fn refuses_an_authenticated_human_who_does_not_hold_the_role() {
    let token = Token {
        roles: vec!["some-other-role".to_owned()],
        ..Token::default()
    };

    assert_eq!(
        authenticator(holder(None)).authenticate(&presenting(&token.encoded())),
        Err(OperatorAuthError::NotAnOperator)
    );
}

#[test]
fn refuses_an_expired_token_beyond_the_leeway() {
    let token = Token {
        expires_in: -120,
        ..Token::default()
    };

    assert_eq!(
        authenticator(holder(None)).authenticate(&presenting(&token.encoded())),
        Err(OperatorAuthError::NotAnOperator)
    );
}

#[test]
fn refuses_a_token_signed_by_a_key_the_provider_does_not_publish() {
    let holder = KeyHolder::empty();
    holder.replace(VerificationKeys::held(vec![(
        None,
        DecodingKey::from_secret(b"a-different-key"),
    )]));

    assert_eq!(
        authenticator(holder).authenticate(&presenting(&Token::default().encoded())),
        Err(OperatorAuthError::NotAnOperator)
    );
}

#[test]
fn refuses_everything_before_the_first_key_set_arrives() {
    assert_eq!(
        authenticator(KeyHolder::empty()).authenticate(&presenting(&Token::default().encoded())),
        Err(OperatorAuthError::NotAnOperator)
    );
}

#[test]
fn a_token_naming_a_key_id_is_checked_against_that_key_alone() {
    let token = Token {
        key_id: Some("rotated-out".to_owned()),
        ..Token::default()
    };

    // The holder publishes the same key under a different id. The signature
    // would verify; the key id says this is not the key that made it.
    assert_eq!(
        authenticator(holder(Some("current"))).authenticate(&presenting(&token.encoded())),
        Err(OperatorAuthError::NotAnOperator)
    );
}

#[test]
fn matches_the_published_key_id_when_the_token_names_one() {
    let token = Token {
        key_id: Some("current".to_owned()),
        ..Token::default()
    };

    assert!(authenticator(holder(Some("current")))
        .authenticate(&presenting(&token.encoded()))
        .is_ok());
}

#[test]
fn a_blank_or_malformed_authorization_header_is_a_missing_identity() {
    for value in ["", "Bearer ", "Basic abc123", "Bearer"] {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_str(value).expect("a legal header value"),
        );

        assert_eq!(
            authenticator(holder(None)).authenticate(&headers),
            Err(OperatorAuthError::Missing),
            "{value:?} should not present an identity"
        );
    }
}

#[test]
fn an_empty_issuer_client_or_role_is_refused_at_construction() {
    for (issuer, client, role) in [("", CLIENT, ROLE), (ISSUER, "  ", ROLE), (ISSUER, CLIENT, "")] {
        assert!(OidcOperators::new(issuer, client, role, KeyHolder::empty(), 60).is_err());
    }
}
