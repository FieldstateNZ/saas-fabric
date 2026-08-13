//! The registered-claim rules applied when signature verification is enabled.

use jsonwebtoken::{Algorithm, Validation};

use crate::readers::LeewaySeconds;

/// The RSA family this client accepts.
const PERMITTED_ALGORITHMS: [Algorithm; 3] = [Algorithm::RS256, Algorithm::RS384, Algorithm::RS512];

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
///
/// Switching `validate_nbf` on is necessary but was never sufficient: this
/// library ignores a `NumericDate` it cannot read, and `nbf` is not one of the
/// claims it requires, so an `nbf` outside `u64` used to constrain nothing here
/// while the canonical posture refused the token. The rule above is upheld by
/// `window`, which both readers run, not by these rules alone.
///
/// The leeway comes from [`LeewaySeconds`] rather than a local constant, so the
/// two postures cannot start out with different windows before a deployment has
/// configured anything.
///
/// # Why `exp` is required here and not in the canonical posture
///
/// `Validation::new` seeds `required_spec_claims` with `exp`. That is restated
/// below rather than inherited, because it is a policy decision and it had gone
/// unwritten: `expiry` accepts a token with no `exp`, on the grounds that which
/// claims a token must carry is the identity platform's business and not the
/// runtime's, and the two statements sat in the codebase contradicting each
/// other with nothing marking which was intended.
///
/// Requiring it is intended, and it is kept. A bearer token with no `exp` never
/// expires, and this is the posture a deployment opts into precisely to have
/// more refused than the architecture strictly requires. Refusing one is the
/// stricter direction, which the rule above permits; it is the *laxer*
/// direction that is forbidden.
///
/// It is also why an `exp` this library cannot read reaches the caller as
/// [`IdentityError::UnverifiedToken`](crate::IdentityError::UnverifiedToken)
/// rather than as an expiry failure. Both `{"exp": -1}` and `{"exp": 1e30}`
/// fail its `NumericDate` parse, and a claim it failed to parse is
/// indistinguishable from one that was never sent, so it reports the required
/// claim missing and `rejection::classify` collapses that into the opaque
/// rejection. The token is refused either way; only the canonical posture,
/// which clamps such values instead of discarding them, can say which end of
/// the window was at fault.
pub(crate) fn baseline() -> Validation {
    let mut validation = Validation::new(Algorithm::RS256);

    validation.algorithms = PERMITTED_ALGORITHMS.to_vec();
    validation.set_required_spec_claims(&["exp"]);
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.leeway = LeewaySeconds::DEFAULT.seconds();

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
    fn an_expiry_claim_is_required_in_this_posture() {
        // Pinned because it is a deliberate divergence from the canonical
        // posture, which accepts a token with no `exp`. Inherited from
        // `Validation::new` and undocumented until now, so an accident and an
        // intention looked identical here.
        assert!(baseline().required_spec_claims.contains("exp"));
    }

    #[test]
    fn no_other_claim_is_required_by_default() {
        // `iss` and `aud` become required only when a deployment configures
        // them, and `nbf` never does — requiring it would reject the ordinary
        // token that simply does not carry one.
        assert_eq!(baseline().required_spec_claims.len(), 1);
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
