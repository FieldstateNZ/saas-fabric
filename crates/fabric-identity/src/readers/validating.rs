//! An optional defence-in-depth reader that verifies signatures.

use jsonwebtoken::{decode, decode_header, Validation};

use crate::readers::{rejection, validation_rules, LeewaySeconds};
use crate::{IdentityError, TokenClaims, TokenReader, VerificationKeys};

/// Verifies a token's signature and registered claims before returning them.
///
/// # When to choose this
///
/// This is **not** the canonical architecture.
/// [`TrustedIngressReader`](crate::TrustedIngressReader) is: the platform edge
/// authenticates, and the runtime consumes the identity it established (§8,
/// §9). SaaS Fabric is authentication-agnostic by design.
///
/// This reader exists for deployments that want a second layer *in addition to*
/// sound network policy — a regulated environment where an auditor expects
/// verification at more than one hop, or a migration period where the ingress
/// guarantee is not yet fully trusted.
///
/// It is deliberately not a substitute for the boundary §9 requires. If an
/// untrusted client can reach the runtime directly, verifying signatures here
/// closes one door in a building with no walls.
///
/// # What it deliberately does not do
///
/// No issuer discovery, no JWKS fetching, no realm knowledge, no provider
/// endpoints. Keys are supplied as a [`VerificationKeys`] snapshot built
/// outside the request path. §24 requires the runtime to stay independent of
/// any identity implementation, and that holds even in this mode: this needs a
/// public key, not a vendor.
///
/// # What it checks
///
/// Signature, the full validity window (`exp` and `nbf`), and — when
/// configured — `iss` and `aud`.
pub struct ValidatingReader {
    keys: VerificationKeys,
    validation: Validation,
}

impl ValidatingReader {
    /// Builds a reader over the given key set.
    #[must_use]
    pub fn new(keys: VerificationKeys) -> Self {
        Self {
            keys,
            validation: validation_rules::baseline(),
        }
    }

    /// Requires tokens to carry one of these issuers.
    #[must_use]
    pub fn with_issuers(mut self, issuers: &[String]) -> Self {
        self.validation.set_issuer(issuers);
        self
    }

    /// Requires tokens to carry one of these audiences.
    #[must_use]
    pub fn with_audiences(mut self, audiences: &[String]) -> Self {
        self.validation.set_audience(audiences);
        self
    }

    /// Overrides the clock-skew allowance.
    ///
    /// Takes the same checked type as
    /// [`TrustedIngressReader::with_leeway`](crate::TrustedIngressReader::with_leeway),
    /// so a deployment cannot widen one posture's window further than the
    /// other's, and cannot widen either far enough to neutralise it.
    #[must_use]
    pub const fn with_leeway(mut self, leeway: LeewaySeconds) -> Self {
        self.validation.leeway = leeway.seconds();
        self
    }
}

impl TokenReader for ValidatingReader {
    fn read(&self, token: &str) -> Result<TokenClaims, IdentityError> {
        let header = decode_header(token).map_err(|_| IdentityError::MalformedToken)?;

        let key = self
            .keys
            .select(header.kid.as_deref())
            .ok_or(IdentityError::UnverifiedToken)?;

        let decoded = decode::<serde_json::Value>(token, key, &self.validation).map_err(|error| {
            // The specific reason goes to the log, never to the caller: telling
            // an attacker which check failed narrows their search for free.
            tracing::debug!(
                event = "identity.token_rejected",
                reason = %error,
                "bearer token failed verification"
            );

            rejection::classify(&error)
        })?;

        match decoded.claims {
            serde_json::Value::Object(object) => Ok(TokenClaims::new(object)),
            _ => Err(IdentityError::MalformedToken),
        }
    }

    fn describe(&self) -> &'static str {
        "validating (defence in depth)"
    }
}
