//! The other end of the token's validity window, mirroring `expiry`.

use fabric_core::Clock;

use crate::readers::LeewaySeconds;
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
/// [`IdentityError::TokenNotYetValid`], for every `nbf` that library can read —
/// `posture_parity_tests` pins that, because the claim was false once already.
/// A fractional `nbf` was enforced there and silently ignored here, which is
/// the wrong way round for the posture that has nothing standing behind it.
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
/// `leeway` allows.
pub(crate) fn ensure_already_valid(
    claims: &TokenClaims,
    clock: &dyn Clock,
    leeway: LeewaySeconds,
) -> Result<(), IdentityError> {
    let Some(not_before) = claims.unix_seconds("nbf") else {
        return Ok(());
    };

    // Unsigned throughout, mirroring `ensure_not_expired`. `saturating_add`
    // cannot fire for any allowance [`LeewaySeconds`] permits, and clamping is
    // the harmless direction if a clock ever reads near the end of time.
    if not_before > clock.now_unix_seconds().saturating_add(leeway.seconds()) {
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

    fn leeway(seconds: u64) -> LeewaySeconds {
        LeewaySeconds::try_new(seconds).unwrap()
    }

    #[test]
    fn accepts_a_token_whose_not_before_has_passed() {
        assert!(ensure_already_valid(&claims(r#"{"nbf":1000}"#), &FrozenClock(5_000), leeway(60)).is_ok());
    }

    #[test]
    fn rejects_a_token_minted_for_later_use() {
        assert_eq!(
            ensure_already_valid(&claims(r#"{"nbf":5000}"#), &FrozenClock(1_000), leeway(60)).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn accepts_a_token_that_becomes_valid_within_the_leeway_window() {
        // 30 seconds early, 60 seconds of allowance: a drifted clock, not a
        // premature token.
        assert!(ensure_already_valid(&claims(r#"{"nbf":1030}"#), &FrozenClock(1_000), leeway(60)).is_ok());
    }

    #[test]
    fn accepts_a_token_exactly_at_the_edge_of_the_leeway_window() {
        // now + leeway == nbf. The comparison is strict, so this is the last
        // accepted second rather than the first rejected one.
        assert!(ensure_already_valid(&claims(r#"{"nbf":1060}"#), &FrozenClock(1_000), leeway(60)).is_ok());
    }

    #[test]
    fn rejects_a_token_one_second_beyond_the_leeway_window() {
        assert_eq!(
            ensure_already_valid(&claims(r#"{"nbf":1061}"#), &FrozenClock(1_000), leeway(60)).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn accepts_a_token_with_no_not_before_claim() {
        assert!(ensure_already_valid(&claims("{}"), &FrozenClock(5_000), leeway(60)).is_ok());
    }

    #[test]
    fn rejects_a_token_whose_fractional_not_before_has_not_arrived() {
        // The reviewer's case: accepted 4000 seconds before it was valid,
        // because a float `nbf` read as no `nbf` at all.
        assert_eq!(
            ensure_already_valid(&claims(r#"{"nbf":5000.0}"#), &FrozenClock(1_000), leeway(60)).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn accepts_a_token_whose_fractional_not_before_has_passed() {
        assert!(ensure_already_valid(&claims(r#"{"nbf":1000.5}"#), &FrozenClock(5_000), leeway(60)).is_ok());
    }

    #[test]
    fn rounding_decides_the_boundary_second() {
        // 1060.4 rounds to 1060, the last accepted second; 1060.5 rounds to
        // 1061, the first rejected one.
        assert!(ensure_already_valid(&claims(r#"{"nbf":1060.4}"#), &FrozenClock(1_000), leeway(60)).is_ok());
        assert_eq!(
            ensure_already_valid(&claims(r#"{"nbf":1060.5}"#), &FrozenClock(1_000), leeway(60)).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn a_not_before_prior_to_the_epoch_has_already_arrived() {
        // Clamped to zero. Unlike `exp`, this end of the window is *supposed*
        // to accept a date in the past, so clamping and ignoring agree here.
        assert!(ensure_already_valid(&claims(r#"{"nbf":-1}"#), &FrozenClock(5_000), leeway(60)).is_ok());
    }

    #[test]
    fn a_not_before_beyond_representable_time_has_not_arrived() {
        // Clamped to `u64::MAX`. Ignoring it would have honoured a token that
        // does not become valid until past the end of representable time.
        assert_eq!(
            ensure_already_valid(&claims(r#"{"nbf":1e30}"#), &FrozenClock(5_000), leeway(60)).unwrap_err(),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn a_clock_near_the_end_of_time_does_not_wrap() {
        // `saturating_add` clamps instead of overflowing, and clamping accepts
        // — the harmless direction, since nothing can be later than `u64::MAX`.
        assert!(ensure_already_valid(&claims(r#"{"nbf":1000}"#), &FrozenClock(u64::MAX), leeway(60)).is_ok());
    }
}
