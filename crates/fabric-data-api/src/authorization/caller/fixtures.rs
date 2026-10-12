//! Callers built through the real identity resolver, for unit tests.

use std::sync::Arc;
use std::time::Instant;

use fabric_core::TenantId;
use fabric_identity::{
    encode_unsigned_token, IdentityConfig, IdentityResolver, TenantIdentity, TrustedIngressReader,
    TrustedIssuer,
};

use crate::{Caller, CallerCredential};

struct FixedClock;

impl fabric_core::Clock for FixedClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_000
    }
}

/// Builds an identity through the real resolver, with `iss` registered to
/// the token's own `tenant_id`, so unit tests exercise production's path.
pub(crate) fn identity_with(claims: serde_json::Value) -> TenantIdentity {
    let serde_json::Value::Object(mut object) = claims else {
        panic!("claims must be an object");
    };
    let tenant = object
        .get("tenant_id")
        .and_then(serde_json::Value::as_str)
        .unwrap()
        .to_owned();
    let issuer = format!("https://identity.test.invalid/realms/{tenant}");
    object.insert("iss".to_owned(), serde_json::Value::String(issuer.clone()));

    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        format!("Bearer {}", encode_unsigned_token(&object))
            .parse()
            .unwrap(),
    );

    let config = IdentityConfig {
        trusted_issuers: vec![TrustedIssuer::new(issuer, TenantId::try_new(&tenant).unwrap())],
        ..IdentityConfig::default()
    };

    IdentityResolver::new(config, Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))))
        .unwrap()
        .resolve(&headers)
        .unwrap()
}

/// A caller over [`identity_with`], with a recognisable credential.
pub(crate) fn caller_with(claims: serde_json::Value) -> Caller {
    Caller {
        identity: identity_with(claims),
        credential: CallerCredential::new("the-callers-own-token".to_owned()),
    }
}
