//! A reader that verifies the token signature. The recommended default.

use jsonwebtoken::{decode, decode_header, Algorithm, Validation};

use crate::{IdentityError, TokenClaims, TokenReader, VerificationKeys};

/// Verifies a token's signature and registered claims before returning them.
///
/// # Why this is the recommended reader
///
/// Specification §9 permits the runtime to trust that the ingress already
/// validated the token, and §11 forbids selecting a tenant with a header so
/// that callers cannot choose their own tenant. Those two together only hold if
/// the token itself is trustworthy inside the trust boundary. Verifying the
/// signature here means an attacker who reaches a runtime pod by some other
/// route — request forgery from a business application, a compromised sidecar,
/// lateral movement — still cannot mint `{"tenant_id":"globex"}` and be
/// believed.
///
/// It costs a public-key verification against keys already in memory. No
/// network call, no identity-provider coupling: this needs a JWKS document, not
/// a vendor (§24). Per §12, verifying a signature does not make the runtime
/// responsible for authentication — it remains a consumer of an established
/// identity, it has simply stopped taking that identity on trust.
///
/// # What it checks
///
/// Signature, `exp`, and — when configured — `iss` and `aud`. Issuer and
/// audience are optional because a deployment may legitimately accept tokens
/// from several brokers, but configuring at least the issuer is strongly
/// advised.
pub struct ValidatingReader {
    keys: VerificationKeys,
    validation: Validation,
}

impl ValidatingReader {
    /// Builds a reader over the given key set.
    ///
    /// Accepts the RSA algorithm family. Permitted algorithms are pinned rather
    /// than taken from the token's own header, which is what stops the classic
    /// downgrade where an attacker sets `alg` to `none` or to an HMAC algorithm
    /// and signs with the public key.
    #[must_use]
    pub fn new(keys: VerificationKeys) -> Self {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.algorithms = vec![Algorithm::RS256, Algorithm::RS384, Algorithm::RS512];
        validation.validate_exp = true;
        validation.leeway = 60;

        Self { keys, validation }
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

    /// Overrides the clock-skew allowance, in seconds.
    #[must_use]
    pub const fn with_leeway_seconds(mut self, leeway_seconds: u64) -> Self {
        self.validation.leeway = leeway_seconds;
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
            // The specific reason goes to the log, not to the caller: telling an
            // attacker which check failed narrows their search for free.
            tracing::debug!(
                event = "identity.token_rejected",
                reason = %error,
                "bearer token failed verification"
            );

            match error.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => IdentityError::ExpiredToken,
                _ => IdentityError::UnverifiedToken,
            }
        })?;

        match decoded.claims {
            serde_json::Value::Object(object) => Ok(TokenClaims::new(object)),
            _ => Err(IdentityError::MalformedToken),
        }
    }

    fn describe(&self) -> &'static str {
        "validating (signature verified)"
    }
}
