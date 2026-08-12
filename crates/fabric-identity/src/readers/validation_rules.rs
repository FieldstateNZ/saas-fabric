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
pub(crate) fn baseline() -> Validation {
    let mut validation = Validation::new(Algorithm::RS256);

    validation.algorithms = PERMITTED_ALGORITHMS.to_vec();
    validation.validate_exp = true;
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
}
