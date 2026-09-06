//! The canonical reader: consume the identity the ingress already established.

use std::sync::Arc;

use fabric_core::Clock;

use crate::readers::jwt_payload::decode_payload;
use crate::readers::{window, LeewaySeconds};
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
/// The token's validity window, at both ends: `exp` and `nbf`. Replaying a
/// captured expired token is cheap, and presenting one minted for later use is
/// cheaper still; refusing either costs one comparison.
///
/// This is the posture where those checks are load-bearing. Because this reader
/// decodes the payload itself rather than handing it to a JWT library, no
/// component upstream of it enforces the window — if this reader skips a check,
/// nothing else performs it.
pub struct TrustedIngressReader {
    clock: Arc<dyn Clock>,
    leeway: LeewaySeconds,
}

impl TrustedIngressReader {
    /// Builds the reader with the default validity-window leeway.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            leeway: LeewaySeconds::DEFAULT,
        }
    }

    /// Overrides the clock-skew allowance.
    ///
    /// One value widens both ends of the validity window, matching how
    /// `jsonwebtoken` treats leeway in the defence-in-depth posture. A skewed
    /// clock is skewed in one direction only, but which direction is not known
    /// in advance, so the allowance covers both.
    ///
    /// The argument is a checked [`LeewaySeconds`] rather than an integer.
    /// This method used to take a bare `i64` and store whatever it was handed,
    /// so a negative value narrowed the window it was supposed to widen and
    /// `i64::MAX` switched off both ends of it — in the one posture where
    /// nothing else performs the check.
    #[must_use]
    pub const fn with_leeway(mut self, leeway: LeewaySeconds) -> Self {
        self.leeway = leeway;
        self
    }
}

impl TokenReader for TrustedIngressReader {
    fn read(&self, token: &str) -> Result<TokenClaims, IdentityError> {
        let claims = decode_payload(token)?;

        // The same function the defence-in-depth posture runs, which is what
        // stops the two drifting apart on the window (see `window`).
        window::ensure_current(&claims, self.clock.as_ref(), self.leeway)?;

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
    fn rejects_a_token_minted_for_later_use() {
        // The canonical posture parses claims itself, so nothing upstream
        // enforces `nbf` on its behalf.
        let premature = token(r#"{"tenant_id":"acme","nbf":5000}"#);

        assert_eq!(
            reader_at(1_000).read(&premature).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn accepts_a_token_whose_not_before_has_already_passed() {
        let mature = token(r#"{"tenant_id":"acme","nbf":1000,"exp":9000}"#);

        assert!(reader_at(5_000).read(&mature).is_ok());
    }

    #[test]
    fn rejects_a_token_minted_for_later_use_with_a_fractional_not_before() {
        // Exactly the token the adversarial review presented: spec-legal under
        // RFC 7519 §2, and accepted 4000 seconds before it became valid.
        let premature = token(r#"{"tenant_id":"acme","nbf":5000.0}"#);

        assert_eq!(
            reader_at(1_000).read(&premature).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn rejects_an_expired_token_with_a_fractional_expiry() {
        // The other half of the same hole: the check nothing upstream repeats.
        let expired = token(r#"{"tenant_id":"acme","exp":1000.0}"#);

        assert_eq!(
            reader_at(5_000).read(&expired).unwrap_err(),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn a_widened_window_still_cannot_be_widened_past_the_ceiling() {
        // The allowance is checked at construction, so there is no value a
        // deployment can supply that switches the window off.
        let reader = reader_at(5_000).with_leeway(LeewaySeconds::try_new(3_600).unwrap());

        assert!(reader.read(&token(r#"{"tenant_id":"acme","exp":2000}"#)).is_ok());
        assert_eq!(
            reader
                .read(&token(r#"{"tenant_id":"acme","exp":1000}"#))
                .unwrap_err(),
            IdentityError::ExpiredToken
        );
        assert!(LeewaySeconds::try_new(u64::MAX).is_err());
    }

    #[test]
    fn the_canonical_posture_does_not_examine_the_signature_or_the_audience() {
        // Accepted here, and refused at the edge — which is the division of
        // labour ADR 0002 chose and ADR 0019 §1 writes down as a contract. The
        // token names an audience belonging to somebody else and carries a
        // signature nothing here could verify; this reader reads claims and
        // checks the window, and that is the whole job.
        //
        // Note this is the *reader*. The resolver refuses an unregistered
        // `iss`, so the pair of tests together says which layer does what.
        let foreign = token(r#"{"tenant_id":"acme","aud":"somebody-else","iss":"https://not.ours"}"#);

        assert!(reader_at(1_000).read(&foreign).is_ok());
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
