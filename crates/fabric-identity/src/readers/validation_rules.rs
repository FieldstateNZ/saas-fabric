//! The registered-claim rules applied when signature verification is enabled.

use jsonwebtoken::{Algorithm, Validation};

/// The RSA family this client accepts.
const PERMITTED_ALGORITHMS: [Algorithm; 3] = [Algorithm::RS256, Algorithm::RS384, Algorithm::RS512];

/// Default clock-skew allowance, in seconds.
pub(crate) const DEFAULT_LEEWAY_SECONDS: u64 = 60;

/// Builds the baseline validation rules.
///
/// Permitted algorithms are **pinned here**, not read from the token's own
/// header. That is what defeats the classic downgrade in which an attacker sets
/// `alg` to `none`, or to an HMAC algorithm signed with the public key. A
/// verifier that trusts the token's claim about how it was signed is not
/// verifying anything.
///
/// `validate_nbf` is set explicitly because `jsonwebtoken` defaults it to
/// `false` — unlike `validate_exp`, which it defaults to `true`. Relying on the
/// default would leave this posture accepting a token minted for later use
/// while the canonical trusted-ingress posture rejected it, which is the wrong
/// way round for a mode whose entire purpose is to check *more*.
pub(crate) fn baseline() -> Validation {
    let mut validation = Validation::new(Algorithm::RS256);

    validation.algorithms = PERMITTED_ALGORITHMS.to_vec();
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.leeway = DEFAULT_LEEWAY_SECONDS;

    validation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_rsa_family_is_permitted() {
        let validation = baseline();

        assert!(validation.algorithms.contains(&Algorithm::RS256));
        assert!(!validation.algorithms.contains(&Algorithm::HS256));
    }

    #[test]
    fn expiry_is_validated_by_default() {
        assert!(baseline().validate_exp);
    }

    #[test]
    fn not_before_is_validated_even_though_the_library_defaults_it_off() {
        // Guards the one rule here that differs from `Validation::default()`,
        // so a future refactor that rebuilds this from the default cannot
        // silently drop it.
        assert!(baseline().validate_nbf);
        assert!(!Validation::new(Algorithm::RS256).validate_nbf);
    }
}
