//! The canonical reader: consume the identity the ingress already established.

use std::sync::Arc;

use fabric_core::Clock;

use crate::readers::expiry::ensure_not_expired;
use crate::readers::jwt_payload::decode_payload;
use crate::{IdentityError, TokenClaims, TokenReader};

/// Reads the claims of a bearer token the platform edge has already validated.
///
/// # This is the normal posture
///
/// It follows the architectural contract directly (§8, §9):
///
/// ```text
/// internet → gateway authenticates and validates the bearer
///          → platform trust boundary
///          → the runtime consumes an established identity
/// ```
///
/// A request that reaches the runtime has already passed a trusted ingress.
/// Re-validating the token here would re-do work the edge exists to do, and
/// would pull identity-provider concerns — issuer discovery, JWKS lifecycle,
/// realm knowledge — into a plane that §24 requires to stay independent of any
/// identity implementation.
///
/// So this reader parses claims and nothing more. Per §12, parsing claims does
/// not make a component responsible for authentication.
///
/// # The boundary this depends on
///
/// The posture is sound because §9 also requires that protected runtime APIs
/// cannot be reached through an untrusted path — `NetworkPolicy`, private
/// cluster networking, service mesh policy, mTLS, or ingress-only exposure.
///
/// If an untrusted client can reach this service directly, that is a network
/// policy failure, and the fix belongs there. Verifying signatures inside the
/// runtime would mask the failure rather than repair it, and would leave every
/// other unauthenticated path into the plane still open.
///
/// [`ValidatingReader`](crate::ValidatingReader) exists for deployments that
/// want signature verification as **defence in depth** — a second layer over
/// sound network policy, not a substitute for it.
///
/// # What is still checked
///
/// Expiry. Replaying a captured expired token is cheap and refusing it costs
/// one comparison.
pub struct TrustedIngressReader {
    clock: Arc<dyn Clock>,
    leeway_seconds: i64,
}

impl TrustedIngressReader {
    /// The default clock-skew allowance applied to `exp`.
    const DEFAULT_LEEWAY_SECONDS: i64 = 60;

    /// Builds the reader with the default expiry leeway.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            leeway_seconds: Self::DEFAULT_LEEWAY_SECONDS,
        }
    }

    /// Overrides the clock-skew allowance applied to `exp`.
    #[must_use]
    pub const fn with_leeway_seconds(mut self, leeway_seconds: i64) -> Self {
        self.leeway_seconds = leeway_seconds;
        self
    }
}

impl TokenReader for TrustedIngressReader {
    fn read(&self, token: &str) -> Result<TokenClaims, IdentityError> {
        let claims = decode_payload(token)?;

        ensure_not_expired(&claims, self.clock.as_ref(), self.leeway_seconds)?;

        Ok(claims)
    }

    fn describe(&self) -> &'static str {
        "trusted-ingress"
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::readers::encode_unsigned_token;

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
    fn reads_the_tenant_claim_from_an_established_identity() {
        let claims = reader_at(1_000).read(&token(r#"{"tenant_id":"acme"}"#)).unwrap();

        assert_eq!(claims.string("tenant_id"), Some("acme"));
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
    fn rejects_a_malformed_token() {
        assert_eq!(
            reader_at(1_000).read("not-a-jwt").unwrap_err(),
            IdentityError::MalformedToken
        );
    }

    #[test]
    fn describes_itself_without_alarm() {
        // Correctly-configured trusted ingress is the normal posture, so the
        // description is neutral rather than a warning.
        assert_eq!(reader_at(1_000).describe(), "trusted-ingress");
    }
}
