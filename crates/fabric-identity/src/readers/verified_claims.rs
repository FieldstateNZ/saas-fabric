//! Turning a presented token into claims the defence-in-depth posture trusts.

use fabric_core::Clock;
use jsonwebtoken::{decode, decode_header, Validation};

use crate::readers::{rejection, window, LeewaySeconds};
use crate::{IdentityError, TokenClaims, VerificationKeys};

/// Verifies a token and returns the claims it carries.
///
/// # The two checks, and why there are two
///
/// First `jsonwebtoken`: signature, the pinned algorithm family, `exp`, `nbf`,
/// and — when the deployment configured them — `iss` and `aud`. Then
/// [`window::ensure_current`], the same validity-window check the canonical
/// trusted-ingress posture runs.
///
/// The second is not redundant. That library discards a `NumericDate` it cannot
/// read rather than enforcing it, and `nbf` is not among the claims it
/// requires, so an `nbf` outside `u64` constrained nothing here while the
/// canonical posture refused the token — the one direction the two postures
/// must never diverge in. `window` carries the full account.
///
/// # Why that order
///
/// A claim-based verdict is only meaningful on a token known to be authentic,
/// so nothing is said about the validity window until the signature has been
/// verified. A caller presenting a forged token is told that it was unverified
/// and nothing more, which is also why the two window errors cannot be used to
/// probe whether a signature was ever checked.
///
/// # Errors
///
/// [`IdentityError::MalformedToken`] if the token is not a readable JWT or its
/// payload is not a JSON object, [`IdentityError::UnverifiedToken`] if no
/// configured key matches or any of the library's checks fail, and the two
/// validity-window errors from [`window::ensure_current`].
pub(crate) fn verify(
    token: &str,
    keys: &VerificationKeys,
    validation: &Validation,
    clock: &dyn Clock,
    leeway: LeewaySeconds,
) -> Result<TokenClaims, IdentityError> {
    let header = decode_header(token).map_err(|_| IdentityError::MalformedToken)?;

    let key = keys
        .select(header.kid.as_deref())
        .ok_or(IdentityError::UnverifiedToken)?;

    let decoded = decode::<serde_json::Value>(token, key, validation).map_err(|error| {
        // The specific reason goes to the log, never to the caller: telling an
        // attacker which check failed narrows their search for free.
        tracing::debug!(
            event = "identity.token_rejected",
            reason = %error,
            "bearer token failed verification"
        );

        rejection::classify(&error)
    })?;

    let serde_json::Value::Object(object) = decoded.claims else {
        return Err(IdentityError::MalformedToken);
    };
    let claims = TokenClaims::new(object);

    window::ensure_current(&claims, clock, leeway)?;

    Ok(claims)
}
