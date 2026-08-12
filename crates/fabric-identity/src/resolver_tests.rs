//! Deriving a tenant identity, and the ways it must refuse.

use std::sync::Arc;
use std::time::Instant;

use fabric_core::Clock;
use http::HeaderMap;
use serde_json::json;

use crate::readers::{encode_unsigned_token, TrustedIngressReader};
use crate::resolver::IdentityResolver;
use crate::{IdentityConfig, IdentityError};

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_000
    }
}

fn resolver() -> IdentityResolver {
    IdentityResolver::new(
        IdentityConfig::default(),
        Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
    )
}

fn headers_for(claims: serde_json::Value) -> HeaderMap {
    let serde_json::Value::Object(object) = claims else {
        panic!("claims must be a JSON object");
    };

    let token = encode_unsigned_token(&object);
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    headers
}

#[test]
fn resolves_the_tenant_from_the_canonical_claim() {
    let headers = headers_for(json!({"sub": "user-123", "tenant_id": "acme", "roles": ["user"]}));
    let identity = resolver().resolve(&headers).unwrap();

    assert_eq!(identity.tenant().as_str(), "acme");
    assert_eq!(identity.subject(), "user-123");
    assert!(identity.has_role("user"));
}

#[test]
fn a_missing_tenant_claim_is_rejected_rather_than_defaulted() {
    let headers = headers_for(json!({"sub": "user-123"}));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::MissingTenantClaim {
            claim: "tenant_id".to_owned()
        }
    );
}

#[test]
fn a_tenant_claim_that_is_not_a_valid_identifier_is_rejected() {
    let headers = headers_for(json!({"tenant_id": "Acme Corp"}));

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::InvalidTenantClaim {
            claim: "tenant_id".to_owned()
        }
    );
}

#[test]
fn a_request_carrying_the_banned_tenant_header_is_rejected() {
    let mut headers = headers_for(json!({"tenant_id": "acme"}));
    headers.insert("x-tenant-id", "globex".parse().unwrap());

    assert_eq!(
        resolver().resolve(&headers).unwrap_err(),
        IdentityError::TenantHeaderPresent {
            header: "x-tenant-id"
        }
    );
}

#[test]
fn the_header_never_overrides_the_token_even_when_it_is_only_ignored() {
    // This is the ambiguous state §11 exists to make impossible: token says
    // acme, header says globex. With rejection switched off the header must
    // still have no effect whatsoever.
    let config = IdentityConfig {
        reject_tenant_header: false,
        ..IdentityConfig::default()
    };
    let resolver =
        IdentityResolver::new(config, Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))));

    let mut headers = headers_for(json!({"tenant_id": "acme"}));
    headers.insert("x-tenant-id", "globex".parse().unwrap());

    assert_eq!(resolver.resolve(&headers).unwrap().tenant().as_str(), "acme");
}

#[test]
fn a_request_with_no_authorization_header_is_rejected() {
    assert_eq!(
        resolver().resolve(&HeaderMap::new()).unwrap_err(),
        IdentityError::MissingAuthorization
    );
}

#[test]
fn a_configurable_claim_name_is_honoured() {
    let config = IdentityConfig {
        tenant_claim: "https://example.com/tenant".to_owned(),
        ..IdentityConfig::default()
    };
    let resolver =
        IdentityResolver::new(config, Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))));

    let headers = headers_for(json!({"https://example.com/tenant": "acme"}));
    assert_eq!(resolver.resolve(&headers).unwrap().tenant().as_str(), "acme");
}
