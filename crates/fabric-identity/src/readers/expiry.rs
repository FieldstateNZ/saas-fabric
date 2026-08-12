//! The one claim check that applies regardless of token-reading posture.

use fabric_core::Clock;

use crate::{IdentityError, TokenClaims};

/// Rejects a token whose `exp` has passed, allowing for clock skew.
///
/// This runs even in the trusted-ingress posture, where signatures are not
/// verified. Replaying a captured expired token is cheap, and refusing it costs
/// one integer comparison — there is no posture in which accepting one is
/// right.
///
/// A token with no `exp` is accepted. The specification does not require one,
/// and which tokens are acceptable is the identity platform's policy to set,
/// not the runtime's.
///
/// # Errors
///
/// [`IdentityError::ExpiredToken`] if `exp` is further in the past than
/// `leeway_seconds` allows.
pub(crate) fn ensure_not_expired(
    claims: &TokenClaims,
    clock: &dyn Clock,
    leeway_seconds: i64,
) -> Result<(), IdentityError> {
    let Some(expiry) = claims.unix_seconds("exp") else {
        return Ok(());
    };

    let now = i64::try_from(clock.now_unix_seconds()).unwrap_or(i64::MAX);

    if now.saturating_sub(leeway_seconds) > expiry {
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

    #[test]
    fn accepts_a_token_that_has_not_expired() {
        assert!(ensure_not_expired(&claims(r#"{"exp":5000}"#), &FrozenClock(1_000), 60).is_ok());
    }

    #[test]
    fn rejects_a_token_that_has_expired() {
        assert_eq!(
            ensure_not_expired(&claims(r#"{"exp":1000}"#), &FrozenClock(5_000), 60).unwrap_err(),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn accepts_a_token_that_expired_within_the_leeway_window() {
        assert!(ensure_not_expired(&claims(r#"{"exp":1000}"#), &FrozenClock(1_030), 60).is_ok());
    }

    #[test]
    fn accepts_a_token_with_no_expiry_claim() {
        assert!(ensure_not_expired(&claims("{}"), &FrozenClock(5_000), 60).is_ok());
    }
}
