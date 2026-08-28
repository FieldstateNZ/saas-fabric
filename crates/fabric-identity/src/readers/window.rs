//! The validity window both postures apply, as one shared pair of checks.

use fabric_core::Clock;

use crate::readers::expiry::ensure_not_expired;
use crate::readers::not_before::ensure_already_valid;
use crate::readers::LeewaySeconds;
use crate::{IdentityError, TokenClaims};

/// Rejects a token that is outside its own validity window at either end.
///
/// # Why both readers call this instead of one of them delegating
///
/// `validation_rules::baseline` states the rule the two postures live under: a
/// posture whose entire purpose is to check *more* must never accept what the
/// canonical one rejects. Leaving the defence-in-depth end of that to
/// `jsonwebtoken` alone made it false.
///
/// That library reads a `NumericDate` through a deserialiser which errors
/// unless the value is finite, non-negative, and below `u64::MAX`; the error is
/// then swallowed into a "failed to parse" marker, and only a *parsed* claim is
/// ever compared. `nbf` is not one of the claims it requires, so an `nbf` it
/// could not read constrained nothing at all — and `{"nbf": 1e30}` was refused
/// by the canonical posture while sailing through the one meant to be stricter.
///
/// Calling the same function from both readers is what makes the rule
/// structural rather than incidental. The defence-in-depth posture runs *these*
/// checks in addition to the library's, so it cannot accept a token the
/// canonical posture refuses unless this function accepts it too — no matter
/// what the library can or cannot parse.
///
/// # Order
///
/// `exp` first, so a replayed credential is reported as expired rather than as
/// premature when a token somehow fails both ends.
///
/// # Errors
///
/// [`IdentityError::ExpiredToken`] if `exp` has passed, or
/// [`IdentityError::TokenNotYetValid`] if `nbf` has not yet arrived, in each
/// case by more than `leeway`.
pub(crate) fn ensure_current(
    claims: &TokenClaims,
    clock: &dyn Clock,
    leeway: LeewaySeconds,
) -> Result<(), IdentityError> {
    ensure_not_expired(claims, clock, leeway)?;
    ensure_already_valid(claims, clock, leeway)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    struct FrozenClock(u64);

    impl Clock for FrozenClock {
        fn now(&self) -> Instant {
            Instant::now()
        }

        fn now_unix_seconds(&self) -> u64 {
            self.0
        }
    }

    fn verdict(json: &str, at: u64) -> Result<(), IdentityError> {
        let claims = TokenClaims::new(serde_json::from_str(json).unwrap());

        ensure_current(&claims, &FrozenClock(at), LeewaySeconds::DEFAULT)
    }

    #[test]
    fn accepts_a_token_inside_its_window_at_both_ends() {
        assert!(verdict(r#"{"nbf":1000,"exp":9000}"#, 5_000).is_ok());
    }

    #[test]
    fn reports_an_expired_token_as_expired() {
        assert_eq!(
            verdict(r#"{"exp":1000}"#, 9_000),
            Err(IdentityError::ExpiredToken)
        );
    }

    #[test]
    fn reports_a_premature_token_as_not_yet_valid() {
        assert_eq!(
            verdict(r#"{"nbf":9000}"#, 1_000),
            Err(IdentityError::TokenNotYetValid)
        );
    }

    #[test]
    fn a_token_failing_both_ends_is_reported_as_expired() {
        // Pins the order. Nonsensical as a claim set, but a token whose `nbf`
        // is after its `exp` is a misconfigured issuer, not an attack, and the
        // answer must at least be stable.
        assert_eq!(
            verdict(r#"{"nbf":9000,"exp":1000}"#, 5_000),
            Err(IdentityError::ExpiredToken)
        );
    }

    #[test]
    fn a_not_before_beyond_the_readable_range_is_still_refused() {
        // The value the defence-in-depth posture used to wave through, now
        // refused by the check both postures share.
        assert_eq!(
            verdict(r#"{"nbf":1e30}"#, 5_000),
            Err(IdentityError::TokenNotYetValid)
        );
    }
}
