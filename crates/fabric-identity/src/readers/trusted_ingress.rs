//! A reader that trusts the ingress and does not verify signatures.

use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use fabric_core::Clock;
use serde_json::{Map, Value};

use crate::{IdentityError, TokenClaims, TokenReader};

/// Decodes token claims **without verifying the signature**.
///
/// # When this is correct
///
/// This is the posture specification §9 describes: a request that reaches the
/// runtime has already passed through a trusted ingress that authenticated the
/// caller and validated the token, so re-validating is redundant work.
///
/// That reasoning holds **only** while §9's other requirement holds — that
/// protected runtime APIs cannot be reached through an untrusted path, enforced
/// by `NetworkPolicy`, mesh policy, mTLS, or equivalent. This reader is sound
/// exactly as far as those controls are.
///
/// # Why [`ValidatingReader`](crate::ValidatingReader) is the better default
///
/// §11 bans the `X-Tenant-Id` header so that a caller cannot choose its own
/// tenant. An unverified token grants precisely the capability the header ban
/// removes: anything that can reach a runtime pod — a server-side request
/// forgery in a business application, a compromised sidecar, lateral movement
/// inside the mesh — can mint `{"tenant_id":"globex"}` and be believed. An
/// unverified token is as caller-controlled as a header; it just looks
/// official.
///
/// Signature verification against keys already in memory costs microseconds and
/// closes that path. It does not make the runtime responsible for
/// authentication (§12 is explicit that parsing claims does not), and it does
/// not couple the runtime to any identity provider (§24) — it needs a public
/// key, not a vendor.
///
/// # What is still checked
///
/// Expiry. A token past its `exp` is rejected even here, because replaying a
/// captured expired token is cheap and refusing it costs one comparison.
pub struct TrustedIngressReader {
    clock: Arc<dyn Clock>,
    leeway_seconds: i64,
}

impl TrustedIngressReader {
    /// Builds the reader with the default 60-second expiry leeway.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            leeway_seconds: 60,
        }
    }

    /// Overrides the clock-skew allowance applied to `exp`.
    #[must_use]
    pub const fn with_leeway_seconds(mut self, leeway_seconds: i64) -> Self {
        self.leeway_seconds = leeway_seconds;
        self
    }

    /// Splits a JWT and returns its payload segment.
    ///
    /// Requires exactly three segments. A two-segment value is not an
    /// unsigned JWT we should tolerate — it is a malformed token.
    fn payload_segment(token: &str) -> Result<&str, IdentityError> {
        let mut segments = token.split('.');

        match (segments.next(), segments.next(), segments.next(), segments.next()) {
            (Some(_header), Some(payload), Some(_signature), None) if !payload.is_empty() => Ok(payload),
            _ => Err(IdentityError::MalformedToken),
        }
    }

    /// Rejects a token whose `exp` has passed, allowing for clock skew.
    ///
    /// A token with no `exp` is accepted: the specification does not require
    /// one, and the ingress is the component responsible for policy about which
    /// tokens are acceptable.
    fn check_expiry(&self, claims: &TokenClaims) -> Result<(), IdentityError> {
        let Some(expiry) = claims.unix_seconds("exp") else {
            return Ok(());
        };

        let now = i64::try_from(self.clock.now_unix_seconds()).unwrap_or(i64::MAX);

        if now.saturating_sub(self.leeway_seconds) > expiry {
            return Err(IdentityError::ExpiredToken);
        }

        Ok(())
    }
}

impl TokenReader for TrustedIngressReader {
    fn read(&self, token: &str) -> Result<TokenClaims, IdentityError> {
        let payload = Self::payload_segment(token)?;

        let decoded = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| IdentityError::MalformedToken)?;

        let value: Value = serde_json::from_slice(&decoded).map_err(|_| IdentityError::MalformedToken)?;

        let Value::Object(object) = value else {
            return Err(IdentityError::MalformedToken);
        };

        let claims = TokenClaims::new(object);
        self.check_expiry(&claims)?;

        Ok(claims)
    }

    fn describe(&self) -> &'static str {
        "trusted-ingress (signature NOT verified)"
    }
}

/// Builds an unsigned-but-well-formed JWT for tests elsewhere in the workspace.
///
/// Lives here rather than in a test module because the Data API integration
/// tests need it too, and duplicating base64 assembly across crates is how the
/// two copies drift apart.
#[must_use]
pub fn encode_unsigned_token(claims: &Map<String, Value>) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(serde_json::Value::Object(claims.clone()).to_string());

    format!("{header}.{payload}.signature-not-verified")
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    /// A clock frozen at a chosen wall-clock second, so expiry tests do not
    /// sleep and do not depend on when they run.
    struct FrozenClock(u64);

    impl Clock for FrozenClock {
        fn now(&self) -> Instant {
            Instant::now()
        }

        fn now_unix_seconds(&self) -> u64 {
            self.0
        }
    }

    fn reader_at(unix_seconds: u64) -> TrustedIngressReader {
        TrustedIngressReader::new(Arc::new(FrozenClock(unix_seconds)))
    }

    fn token(json: &str) -> String {
        encode_unsigned_token(&serde_json::from_str(json).unwrap())
    }

    #[test]
    fn reads_the_tenant_claim_from_a_well_formed_token() {
        let claims = reader_at(1_000).read(&token(r#"{"tenant_id":"acme"}"#)).unwrap();
        assert_eq!(claims.string("tenant_id"), Some("acme"));
    }

    #[test]
    fn rejects_a_token_that_is_not_three_segments() {
        assert_eq!(
            reader_at(1_000).read("only.two").unwrap_err(),
            IdentityError::MalformedToken
        );
    }

    #[test]
    fn rejects_a_payload_that_is_not_base64() {
        assert_eq!(
            reader_at(1_000)
                .read("header.!!!not-base64!!!.signature")
                .unwrap_err(),
            IdentityError::MalformedToken
        );
    }

    #[test]
    fn rejects_a_payload_that_is_json_but_not_an_object() {
        let payload = URL_SAFE_NO_PAD.encode("[1,2,3]");
        assert_eq!(
            reader_at(1_000).read(&format!("h.{payload}.s")).unwrap_err(),
            IdentityError::MalformedToken
        );
    }

    #[test]
    fn rejects_an_expired_token_even_though_it_does_not_verify_signatures() {
        let expired = token(r#"{"tenant_id":"acme","exp":1000}"#);
        assert_eq!(
            reader_at(5_000).read(&expired).unwrap_err(),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn accepts_a_token_that_expired_within_the_leeway_window() {
        let expired = token(r#"{"tenant_id":"acme","exp":1000}"#);
        assert!(reader_at(1_030).read(&expired).is_ok());
    }

    #[test]
    fn accepts_a_token_with_no_expiry_claim() {
        assert!(reader_at(5_000).read(&token(r#"{"tenant_id":"acme"}"#)).is_ok());
    }
}
