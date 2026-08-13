//! The closing end of the token's validity window. See `not_before` for the
//! opening end, which mirrors the skew convention applied here.

use fabric_core::Clock;

use crate::readers::LeewaySeconds;
use crate::{IdentityError, TokenClaims};

/// Rejects a token whose `exp` has passed, allowing for clock skew.
///
/// This runs even in the trusted-ingress posture, where signatures are not
/// verified. Replaying a captured expired token is cheap, and refusing it costs
/// one integer comparison — there is no posture in which accepting one is
/// right.
///
/// A token with no `exp` is accepted **by this function**. The specification
/// does not require one, and which tokens are acceptable is the identity
/// platform's policy to set, not the runtime's. A token that *has* one is a
/// different matter: every number a claim can hold yields a second (see
/// [`TokenClaims::unix_seconds`]), so a present `exp` always constrains.
///
/// The defence-in-depth posture goes further and requires `exp`, so a token
/// without one is refused there and accepted here. That is deliberate, and
/// `validation_rules::baseline` records why — a posture chosen to be stricter
/// may reject more, so long as it never accepts more.
///
/// # Errors
///
/// [`IdentityError::ExpiredToken`] if `exp` is further in the past than
/// `leeway` allows.
pub(crate) fn ensure_not_expired(
    claims: &TokenClaims,
    clock: &dyn Clock,
    leeway: LeewaySeconds,
) -> Result<(), IdentityError> {
    let Some(expiry) = claims.unix_seconds("exp") else {
        return Ok(());
    };

    // Unsigned throughout, matching both the clock and `jsonwebtoken`'s own
    // comparison, so there is no conversion to get wrong. `saturating_sub`
    // covers a clock reading earlier than the allowance itself, which yields
    // zero — no token is expired before the epoch.
    if clock.now_unix_seconds().saturating_sub(leeway.seconds()) > expiry {
        return Err(IdentityError::ExpiredToken);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    /// A clock frozen at a chosen second, so expiry tests neither sleep nor
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
    fn accepts_a_token_that_has_not_expired() {
        assert!(ensure_not_expired(&claims(r#"{"exp":5000}"#), &FrozenClock(1_000), leeway(60)).is_ok());
    }

    #[test]
    fn rejects_a_token_that_has_expired() {
        assert_eq!(
            ensure_not_expired(&claims(r#"{"exp":1000}"#), &FrozenClock(5_000), leeway(60)).unwrap_err(),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn accepts_a_token_that_expired_within_the_leeway_window() {
        assert!(ensure_not_expired(&claims(r#"{"exp":1000}"#), &FrozenClock(1_030), leeway(60)).is_ok());
    }

    #[test]
    fn accepts_a_token_with_no_expiry_claim() {
        assert!(ensure_not_expired(&claims("{}"), &FrozenClock(5_000), leeway(60)).is_ok());
    }

    #[test]
    fn rejects_a_token_whose_fractional_expiry_has_passed() {
        // The reviewer's case. `exp` is spec-legal as a float, and reading it
        // as absent used to accept a token that expired 4000 seconds earlier.
        assert_eq!(
            ensure_not_expired(&claims(r#"{"exp":1000.0}"#), &FrozenClock(5_000), leeway(60)).unwrap_err(),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn accepts_a_token_whose_fractional_expiry_is_still_ahead() {
        assert!(ensure_not_expired(&claims(r#"{"exp":5000.5}"#), &FrozenClock(1_000), leeway(60)).is_ok());
    }

    #[test]
    fn accepts_a_token_exactly_at_the_edge_of_the_leeway_window() {
        // now - leeway == exp. The comparison is strict, so this is the last
        // accepted second rather than the first rejected one.
        assert!(ensure_not_expired(&claims(r#"{"exp":1000}"#), &FrozenClock(1_060), leeway(60)).is_ok());
    }

    #[test]
    fn rejects_a_token_one_second_beyond_the_leeway_window() {
        assert_eq!(
            ensure_not_expired(&claims(r#"{"exp":1000}"#), &FrozenClock(1_061), leeway(60)).unwrap_err(),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn rounding_decides_the_boundary_second() {
        // 1060.5 rounds to 1061, one second inside the window; 1060.4 rounds
        // to 1060, the last second outside it.
        assert!(ensure_not_expired(&claims(r#"{"exp":1060.5}"#), &FrozenClock(1_121), leeway(60)).is_ok());
        assert_eq!(
            ensure_not_expired(&claims(r#"{"exp":1060.4}"#), &FrozenClock(1_121), leeway(60)).unwrap_err(),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn an_expiry_before_the_epoch_is_expired_rather_than_ignored() {
        // Clamped to zero, which is the honest reading: an instant in 1969 has
        // long passed. Treating it as absent would have been a free pass.
        assert_eq!(
            ensure_not_expired(&claims(r#"{"exp":-1}"#), &FrozenClock(5_000), leeway(60)).unwrap_err(),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn an_expiry_beyond_representable_time_has_not_passed() {
        assert!(ensure_not_expired(&claims(r#"{"exp":1e30}"#), &FrozenClock(5_000), leeway(60)).is_ok());
    }

    #[test]
    fn a_clock_earlier_than_the_allowance_itself_does_not_wrap() {
        // `saturating_sub` floors at zero. Without it this would underflow, and
        // a wrapped `now` would make every token look expired.
        assert!(ensure_not_expired(&claims(r#"{"exp":0}"#), &FrozenClock(1), leeway(60)).is_ok());
    }
}
