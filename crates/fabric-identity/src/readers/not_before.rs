//! The other end of the token's validity window, mirroring `expiry`.

use fabric_core::Clock;

use crate::{IdentityError, TokenClaims};

/// Rejects a token whose `nbf` has not yet arrived, allowing for clock skew.
///
/// # Why this is a live check, not a formality
///
/// It runs in the canonical trusted-ingress posture, which is exactly where it
/// matters. That posture parses claims itself rather than delegating to a JWT
/// library, so `nbf` is only enforced if this crate enforces it — nothing
/// upstream does it on our behalf. A token minted now for use next week is a
/// perfectly ordinary thing for an identity provider to issue, and without this
/// check the runtime would honour it immediately.
///
/// The defence-in-depth [`ValidatingReader`](crate::ValidatingReader) does not
/// call this; it gets the same check from `jsonwebtoken`, which needs
/// `validate_nbf` switched on explicitly because it defaults to off. Both
/// postures therefore reject the same tokens with the same
/// [`IdentityError::TokenNotYetValid`].
///
/// # Skew
///
/// The allowance is deliberately the mirror of `ensure_not_expired`'s, so one
/// `leeway_seconds` widens the validity window symmetrically at both ends:
/// expiry accepts while `now <= exp + leeway`, and this accepts once
/// `now >= nbf - leeway`. Applying skew to only one end would mean a clock
/// drifted far enough to matter was tolerated in one direction and not the
/// other.
///
/// A token with no `nbf` is accepted, for the same reason a token with no `exp`
/// is: which claims a token must carry is the identity platform's policy, not
/// the runtime's.
///
/// # Errors
///
/// [`IdentityError::TokenNotYetValid`] if `nbf` is further in the future than
/// `leeway_seconds` allows.
pub(crate) fn ensure_already_valid(
    claims: &TokenClaims,
    clock: &dyn Clock,
    leeway_seconds: i64,
) -> Result<(), IdentityError> {
    let Some(not_before) = claims.unix_seconds("nbf") else {
        return Ok(());
    };

    let now = i64::try_from(clock.now_unix_seconds()).unwrap_or(i64::MAX);

    if not_before > now.saturating_add(leeway_seconds) {
        return Err(IdentityError::TokenNotYetValid);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    /// A clock frozen at a chosen second, so these tests neither sleep nor
    /// depend on when they run.
    struct FrozenClock(u64);

    impl Clock for FrozenClock {
        fn now(&self) -> Instant {
            Instant::now()
        }

        fn now_unix_seconds(&self) -> u64 {
            self.0
        }
    }

    fn claims(json: &str) -> TokenClaims {
        TokenClaims::new(serde_json::from_str(json).unwrap())
    }

    #[test]
    fn accepts_a_token_whose_not_before_has_passed() {
        assert!(ensure_already_valid(&claims(r#"{"nbf":1000}"#), &FrozenClock(5_000), 60).is_ok());
    }

    #[test]
    fn rejects_a_token_minted_for_later_use() {
        assert_eq!(
            ensure_already_valid(&claims(r#"{"nbf":5000}"#), &FrozenClock(1_000), 60).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn accepts_a_token_that_becomes_valid_within_the_leeway_window() {
        // 30 seconds early, 60 seconds of allowance: a drifted clock, not a
        // premature token.
        assert!(ensure_already_valid(&claims(r#"{"nbf":1030}"#), &FrozenClock(1_000), 60).is_ok());
    }

    #[test]
    fn accepts_a_token_exactly_at_the_edge_of_the_leeway_window() {
        // now + leeway == nbf. The comparison is strict, so this is the last
        // accepted second rather than the first rejected one.
        assert!(ensure_already_valid(&claims(r#"{"nbf":1060}"#), &FrozenClock(1_000), 60).is_ok());
    }

    #[test]
    fn rejects_a_token_one_second_beyond_the_leeway_window() {
        assert_eq!(
            ensure_already_valid(&claims(r#"{"nbf":1061}"#), &FrozenClock(1_000), 60).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn accepts_a_token_with_no_not_before_claim() {
        assert!(ensure_already_valid(&claims("{}"), &FrozenClock(5_000), 60).is_ok());
    }
}
