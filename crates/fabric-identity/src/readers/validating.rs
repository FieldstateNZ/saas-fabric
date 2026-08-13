//! An optional defence-in-depth reader that verifies signatures.

use std::sync::Arc;

use fabric_core::{Clock, SystemClock};
use jsonwebtoken::Validation;

use crate::readers::{validation_rules, verified_claims, LeewaySeconds};
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
/// guarantee is not yet fully trusted. It is not a substitute for the boundary
/// §9 requires: if an untrusted client can reach the runtime directly,
/// verifying signatures here closes one door in a building with no walls.
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
/// The signature and `jsonwebtoken`'s registered-claim rules, and then the same
/// validity window the canonical posture applies. `verified_claims` holds that
/// pipeline and explains why both halves are needed and why they run in that
/// order; this type is only the configuration around it.
pub struct ValidatingReader {
    keys: VerificationKeys,
    validation: Validation,

    /// Drives the shared window check. `jsonwebtoken` reads the system clock
    /// directly and offers no seam, so in production this reads the same clock;
    /// the field exists so `posture_parity_tests` can freeze it.
    clock: Arc<dyn Clock>,

    /// Holds the same number as `validation.leeway`, which the library uses for
    /// its own comparisons. [`Self::with_leeway`] sets both, so the two halves
    /// of this reader cannot end up with different windows.
    leeway: LeewaySeconds,
}

impl ValidatingReader {
    /// Builds a reader over the given key set.
    #[must_use]
    pub fn new(keys: VerificationKeys) -> Self {
        Self {
            keys,
            validation: validation_rules::baseline(),
            clock: SystemClock::shared(),
            leeway: LeewaySeconds::DEFAULT,
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

    /// Overrides the clock-skew allowance, for both halves of this reader.
    ///
    /// Takes the same checked type as
    /// [`TrustedIngressReader::with_leeway`](crate::TrustedIngressReader::with_leeway),
    /// so a deployment cannot widen one posture's window further than the
    /// other's, and cannot widen either far enough to neutralise it.
    #[must_use]
    pub const fn with_leeway(mut self, leeway: LeewaySeconds) -> Self {
        self.validation.leeway = leeway.seconds();
        self.leeway = leeway;
        self
    }
}

impl TokenReader for ValidatingReader {
    fn read(&self, token: &str) -> Result<TokenClaims, IdentityError> {
        verified_claims::verify(
            token,
            &self.keys,
            &self.validation,
            self.clock.as_ref(),
            self.leeway,
        )
    }

    fn describe(&self) -> &'static str {
        "validating (defence in depth)"
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a real reader with signature verification switched off, for
    /// `posture_parity_tests`.
    ///
    /// Everything that decides *this* crate's question — the rules, the error
    /// mapping, the shared window check, and the order they run in — is the
    /// production path, because this returns an ordinary [`ValidatingReader`].
    /// Only the signature is skipped, and only because signing fixtures would
    /// mean committing an RSA private key.
    ///
    /// Note that `insecure_disable_signature_validation` switches off more than
    /// its name suggests: `jsonwebtoken`'s `decoding.rs` gates the algorithm
    /// allowlist *and* the key-family check on the same flag. That is why an
    /// HS256 fixture verifies against these RS-only rules.
    pub(crate) fn insecure_reader(secret: &[u8], clock: Arc<dyn Clock>) -> ValidatingReader {
        let mut reader = ValidatingReader::new(VerificationKeys::from_shared_secret(secret));

        reader.validation.insecure_disable_signature_validation();
        reader.clock = clock;

        reader
    }
}
