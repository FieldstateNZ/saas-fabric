//! What a secret boundary accepts, and what it refuses.

use crate::SecretNamespace;

#[test]
fn a_boundary_takes_the_same_rule_a_realm_does() {
    assert!(SecretNamespace::try_new("acme").is_ok());
    assert!(SecretNamespace::try_new("acme-holdings").is_ok());

    // Uppercase is refused rather than folded. `Acme` and `acme` being the
    // same boundary at one layer and different at another is exactly the trap
    // this rule exists to avoid.
    assert!(SecretNamespace::try_new("Acme").is_err());
    assert!(SecretNamespace::try_new("acme/evil").is_err());
    assert!(SecretNamespace::try_new("").is_err());
}

#[test]
fn a_boundary_cannot_carry_a_path_separator() {
    // The one that matters: a boundary containing a separator would address a
    // different boundary once interpolated.
    for attempt in ["acme/other", "../other", "acme%2fother", "acme other"] {
        assert!(
            SecretNamespace::try_new(attempt).is_err(),
            "{attempt} must not be a boundary"
        );
    }
}
