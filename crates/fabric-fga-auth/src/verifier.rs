//! Turning a presented token into a verified identity, or refusing it.

mod unverified;
#[cfg(test)]
mod verifier_tests;

use std::sync::Arc;

use fabric_core::SubjectId;
use jsonwebtoken::{decode, decode_header, Validation};
use serde::Deserialize;

use crate::{KeyCache, RefusalReason, Registry, VerificationError, VerifiedIdentity};

/// How far outside its validity window a token is still accepted.
///
/// Set explicitly because the library's default is **60 seconds**, and a
/// minute in which an expired token still verifies is a security property
/// nobody chose. Some tolerance is genuinely needed — the provider's clock and
/// this process's clock are not the same clock — so the answer is a small
/// deliberate number rather than zero.
///
/// If a deployment ever needs a different one, it belongs in the registration
/// beside the algorithms, for the same reason those are there.
const CLOCK_SKEW_TOLERANCE_SECONDS: u64 = 30;

/// Verifies tenant users' tokens against a trusted issuer registry.
pub struct Verifier {
    /// The issuers Fabric trusts. Never empty — see [`Registry::build`].
    registry: Registry,

    /// The keys currently trusted for each of them.
    keys: Arc<KeyCache>,
}

impl Verifier {
    /// Builds a verifier over a registry and a key cache.
    #[must_use]
    pub const fn new(registry: Registry, keys: Arc<KeyCache>) -> Self {
        Self { registry, keys }
    }

    /// Verifies a token and says who presented it.
    ///
    /// The order matters and is not an implementation detail: the issuer
    /// selects a registration *before* any signature work, because which key
    /// and which algorithm are acceptable are properties of the registration
    /// rather than of the token.
    ///
    /// # Errors
    ///
    /// [`VerificationError::Refused`] for a credential this verifier will not
    /// accept, and [`VerificationError::Unavailable`] when trust could not be
    /// established — which is never the caller's fault and never a `401`.
    pub async fn verify(&self, token: &str) -> Result<VerifiedIdentity, VerificationError> {
        let issuer = unverified::issuer_of(token).map_err(VerificationError::Refused)?;

        let registration = self
            .registry
            .registration(&issuer)
            .ok_or(VerificationError::Refused(RefusalReason::UnknownIssuer))?;

        let header =
            decode_header(token).map_err(|_| VerificationError::Refused(RefusalReason::Malformed))?;

        // Pinned per issuer, so the header cannot nominate the cryptography.
        // Refusing `alg: none` alone would not be enough: a token perfectly
        // signed with an algorithm nobody agreed to is still not acceptable.
        if !registration.permits(header.alg) {
            return Err(VerificationError::Refused(RefusalReason::DisallowedAlgorithm));
        }

        let key_id = header
            .kid
            .ok_or(VerificationError::Refused(RefusalReason::Malformed))?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&registration.issuer]);
        validation.set_audience(&[&registration.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.leeway = CLOCK_SKEW_TOLERANCE_SECONDS;

        let claims = self
            .keys
            .with_key(registration, &key_id, |key| {
                decode::<VerifiedClaims>(token, key, &validation)
            })
            .await?
            .map_err(|error| VerificationError::Refused(refusal_for(&error)))?
            .claims;

        let subject = claims
            .sub
            .ok_or(VerificationError::Refused(RefusalReason::NoSubject))?;

        // The realm is the registration's tenant, never anything the token
        // said — which is the invariant `from_verified` is named for.
        let principal = SubjectId::from_verified(&registration.tenant, &subject)
            .map_err(|_| VerificationError::Refused(RefusalReason::UnusableSubject))?;

        Ok(VerifiedIdentity::new(
            registration.tenant.clone(),
            subject,
            principal,
            registration.store.clone(),
            registration.authorization_model_id.clone(),
        ))
    }
}

/// Which refusal a library error actually describes.
fn refusal_for(error: &jsonwebtoken::errors::Error) -> RefusalReason {
    use jsonwebtoken::errors::ErrorKind;

    match error.kind() {
        ErrorKind::ExpiredSignature | ErrorKind::ImmatureSignature => RefusalReason::OutsideValidity,
        ErrorKind::InvalidAudience => RefusalReason::WrongAudience,
        ErrorKind::InvalidIssuer => RefusalReason::UnknownIssuer,
        ErrorKind::InvalidAlgorithm => RefusalReason::DisallowedAlgorithm,
        _ => RefusalReason::BadSignature,
    }
}

/// The claims read only after the signature has been checked.
#[derive(Deserialize)]
struct VerifiedClaims {
    /// The subject this token was issued for.
    sub: Option<String>,
}
