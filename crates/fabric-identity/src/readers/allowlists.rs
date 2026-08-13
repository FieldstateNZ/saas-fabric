//! Making a configured issuer or audience allowlist actually refuse something.

use jsonwebtoken::Validation;

/// Applies an issuer allowlist and makes `iss` mandatory.
///
/// The second half is the load-bearing one. `Validation::set_issuer` records
/// the accepted values and nothing else: `required_spec_claims` is left as
/// `baseline` left it, and `validate`'s `iss` arm matches only when the claim
/// parsed, so a token with no `iss` falls through its catch-all and is
/// accepted. An allowlist that any caller bypasses by omitting the claim is a
/// control that reads as enabled while doing nothing at all.
///
/// An empty slice therefore configures an allowlist that nothing matches, and
/// refuses every token. That is the safe direction rather than the useful one —
/// callers that mean "no allowlist" must not call this at all, which is what
/// `fabric-api`'s composition root does when its configuration lists none.
pub(crate) fn require_issuers(validation: &mut Validation, issuers: &[String]) {
    validation.set_issuer(issuers);
    require_claim(validation, "iss");
}

/// Applies an audience allowlist, makes `aud` mandatory, and switches the
/// audience check back on.
///
/// The omission bypass is the same one [`require_issuers`] describes. The third
/// step is what keeps this composing with `baseline`, which switches
/// `validate_aud` **off** so an unconfigured deployment does not refuse every
/// token that merely carries an `aud`. That default has to be undone here or a
/// configured allowlist would be silently ignored — trading one hole for
/// another rather than closing either.
///
/// The empty-slice caveat on [`require_issuers`] applies here too.
pub(crate) fn require_audiences(validation: &mut Validation, audiences: &[String]) {
    validation.set_audience(audiences);
    validation.validate_aud = true;
    require_claim(validation, "aud");
}

/// Adds one claim to the set a token must carry, keeping whatever is already
/// there.
///
/// `set_required_spec_claims` *replaces* the set rather than extending it, so
/// calling it with only the new claim would drop the `exp` that `baseline`
/// deliberately requires. Reading the current set first is also what makes the
/// two functions above order-independent: configuring both allowlists, in
/// either order, leaves all three claims required.
fn require_claim(validation: &mut Validation, claim: &str) {
    let mut required: Vec<String> = validation.required_spec_claims.iter().cloned().collect();

    required.push(claim.to_owned());
    validation.set_required_spec_claims(&required);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::readers::validation_rules;

    fn required(validation: &Validation) -> Vec<String> {
        let mut claims: Vec<String> = validation.required_spec_claims.iter().cloned().collect();

        claims.sort();
        claims
    }

    #[test]
    fn an_issuer_allowlist_makes_the_issuer_claim_required() {
        let mut validation = validation_rules::baseline();

        require_issuers(&mut validation, &["https://trusted.example".to_owned()]);

        assert_eq!(required(&validation), ["exp", "iss"]);
    }

    #[test]
    fn an_audience_allowlist_makes_the_audience_claim_required_and_checked() {
        let mut validation = validation_rules::baseline();

        require_audiences(&mut validation, &["saas-fabric".to_owned()]);

        assert_eq!(required(&validation), ["aud", "exp"]);
        // Without this the allowlist above would be configured and ignored.
        assert!(validation.validate_aud);
    }

    #[test]
    fn configuring_both_in_either_order_requires_all_three_claims() {
        // The composition the replacing semantics of `set_required_spec_claims`
        // would otherwise break: whichever call ran second would drop the
        // claim the first one added, along with `exp`.
        for reversed in [false, true] {
            let mut validation = validation_rules::baseline();
            let issuers = ["https://trusted.example".to_owned()];
            let audiences = ["saas-fabric".to_owned()];

            if reversed {
                require_audiences(&mut validation, &audiences);
                require_issuers(&mut validation, &issuers);
            } else {
                require_issuers(&mut validation, &issuers);
                require_audiences(&mut validation, &audiences);
            }

            assert_eq!(required(&validation), ["aud", "exp", "iss"]);
        }
    }
}
