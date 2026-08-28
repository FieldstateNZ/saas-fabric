//! Holds the two postures against each other on the token's validity window.
//!
//! # Why this file exists
//!
//! `not_before.rs` asserts in its rustdoc that both postures "reject the same
//! tokens with the same `IdentityError::TokenNotYetValid`". That sentence was
//! false when it was written: a spec-legal fractional `nbf` was enforced by
//! `jsonwebtoken` and silently ignored by the canonical reader, which is the
//! wrong way round — the canonical posture is the one with nothing standing
//! behind it. Nothing failed, because no test ever compared the two.
//!
//! So the claim is pinned here rather than restated in prose.
//!
//! # What is compared, and what is not
//!
//! The axis under test is how each posture interprets a `NumericDate` and turns
//! it into a verdict. Both sides are driven through the real readers, so
//! nothing about the rules, the error mapping, the shared window check, or the
//! order they run in is restated here — a parity test that restated any of them
//! would keep passing while the real ones drifted, which is the failure this
//! file exists to prevent. An earlier version of this file did compose those
//! pieces by hand, and the composition it chose was not the one production ran.
//!
//! The single deviation is that the defence-in-depth reader has signature
//! verification switched off, because signing fixtures would mean committing an
//! RSA private key. Signature verification is a separate axis with its own
//! tests.
//!
//! That switch is broader than its name: `jsonwebtoken` gates the algorithm
//! allowlist and the key-family check on the same `validate_signature` flag
//! (`decoding.rs`), so disabling it also disables the RS-only pin. That is why
//! an HS256 fixture is readable by rules that permit only the RSA family, and
//! it is worth knowing before reading a passing test as evidence that the pin
//! works — it is not.
//!
//! # Why the clock is real
//!
//! `jsonwebtoken` reads the system clock directly and offers no seam, so every
//! instant here is expressed relative to real "now" and the canonical reader is
//! given a clock frozen at that same second. The offsets are hours wide, so a
//! test that straddles a second boundary still lands on the same verdict.

use std::sync::Arc;

use fabric_core::{Clock, SystemClock};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};

use crate::readers::{validating, validation_rules, LeewaySeconds, TrustedIngressReader};
use crate::{IdentityError, TokenReader};

/// The secret both the fixture signer and the reader are given.
const FIXTURE_SECRET: &[u8] = b"parity-fixture";

/// A second comfortably inside both postures' notion of the present.
fn now() -> u64 {
    SystemClock.now_unix_seconds()
}

/// A second as a float, so a fractional `NumericDate` can be built from it.
///
/// Unix seconds sit around 2^31, far inside the 2^53 range an `f64` holds
/// exactly, so the cast loses nothing at any date these tests can reach.
fn as_float(seconds: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let value = seconds as f64;

    value
}

/// A clock frozen at a chosen second, so the canonical posture is asked about
/// the same instant `jsonwebtoken` will read from the system.
struct FrozenClock(u64);

impl Clock for FrozenClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        self.0
    }
}

/// Signs a claim set into a real JWT.
///
/// HS256 because the signature is not what is being tested and the algorithm
/// pin is bypassed along with verification; the token still has to be a
/// genuinely well-formed JWT for either posture to read it.
fn token(claims: &Value) -> String {
    encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(FIXTURE_SECRET),
    )
    .unwrap()
}

/// The canonical trusted-ingress verdict on a token.
fn trusted_ingress_verdict(token: &str, at: u64) -> Result<(), IdentityError> {
    TrustedIngressReader::new(Arc::new(FrozenClock(at)))
        .read(token)
        .map(|_| ())
}

/// The defence-in-depth verdict, through the production reader.
fn validating_verdict(token: &str, at: u64) -> Result<(), IdentityError> {
    validating::tests::insecure_reader(FIXTURE_SECRET, Arc::new(FrozenClock(at)))
        .read(token)
        .map(|_| ())
}

/// Asserts that both postures reach `expected` on the same token.
///
/// Comparing them to each other as well as to `expected` is deliberate: a
/// regression that broke *both* in the same direction would still be a
/// regression, and asserting only equality would let it through.
#[track_caller]
fn assert_both_postures(claims: &Value, expected: &Result<(), IdentityError>) {
    let at = now();
    let token = token(claims);

    let canonical = trusted_ingress_verdict(&token, at);
    let defence_in_depth = validating_verdict(&token, at);

    assert_eq!(
        canonical, defence_in_depth,
        "the postures disagreed about {claims}: trusted-ingress said {canonical:?}, \
         defence-in-depth said {defence_in_depth:?}"
    );
    assert_eq!(&canonical, expected, "both postures were wrong about {claims}");
}

/// Asserts a divergence that is deliberate, documented, and in the safe
/// direction.
///
/// The rule the postures live under is one-directional: the defence-in-depth
/// one may refuse what the canonical one accepts, never the reverse. The rows
/// asserted through here are that permitted direction, so they are pinned
/// rather than closed — a change to either side has to come to this file and
/// say which row moved and why.
///
/// The final assertion is the rule itself, and is what keeps this helper from
/// being a way to bless a divergence in the dangerous direction.
#[track_caller]
fn assert_defence_in_depth_is_stricter(
    claims: &Value,
    canonical_expected: &Result<(), IdentityError>,
    defence_expected: &Result<(), IdentityError>,
) {
    let at = now();
    let token = token(claims);

    let canonical = trusted_ingress_verdict(&token, at);
    let defence_in_depth = validating_verdict(&token, at);

    assert_eq!(
        &canonical, canonical_expected,
        "trusted-ingress moved on {claims}"
    );
    assert_eq!(
        &defence_in_depth, defence_expected,
        "defence-in-depth moved on {claims}"
    );
    assert!(
        defence_in_depth.is_err(),
        "{claims} is recorded as a stricter-direction divergence, but defence-in-depth accepted it"
    );
}

#[test]
fn a_fractional_not_before_in_the_future_is_refused_by_both_postures() {
    // The claim that was false: `jsonwebtoken` rounded this into a `u64` and
    // rejected it, while the canonical reader read it as absent and let the
    // token through four thousand seconds early.
    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": as_float(now()) + 10_000.5, "exp": now() + 20_000 }),
        &Err(IdentityError::TokenNotYetValid),
    );
}

#[test]
fn a_fractional_expiry_in_the_past_is_refused_by_both_postures() {
    assert_both_postures(
        &json!({ "tenant_id": "acme", "exp": as_float(now()) - 10_000.5 }),
        &Err(IdentityError::ExpiredToken),
    );
}

#[test]
fn a_fractional_not_before_that_has_passed_is_accepted_by_both_postures() {
    // Parity has to hold on the accepting side too, or the fix would just be a
    // different disagreement.
    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": as_float(now()) - 10_000.5, "exp": now() + 20_000 }),
        &Ok(()),
    );
}

#[test]
fn a_fractional_expiry_still_in_the_future_is_accepted_by_both_postures() {
    assert_both_postures(
        &json!({ "tenant_id": "acme", "exp": as_float(now()) + 10_000.5 }),
        &Ok(()),
    );
}

#[test]
fn whole_second_dates_agree_too_so_the_fix_did_not_move_the_ordinary_case() {
    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": now() + 10_000, "exp": now() + 20_000 }),
        &Err(IdentityError::TokenNotYetValid),
    );
    assert_both_postures(
        &json!({ "tenant_id": "acme", "exp": now() - 10_000 }),
        &Err(IdentityError::ExpiredToken),
    );
    assert_both_postures(&json!({ "tenant_id": "acme", "exp": now() + 10_000 }), &Ok(()));
}

#[test]
fn the_rounding_boundary_falls_on_the_same_second_in_both_postures() {
    // Half away from zero, so `.5` rounds up. At `now + leeway + 0.5` the date
    // rounds to `now + leeway + 1`, one second past the strict comparison in
    // both implementations, and both must refuse it.
    let boundary = now() + LeewaySeconds::DEFAULT.seconds();

    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": as_float(boundary) + 0.5, "exp": now() + 20_000 }),
        &Err(IdentityError::TokenNotYetValid),
    );

    // And at `.4` it rounds back down onto the last accepted second.
    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": as_float(boundary) - 0.4, "exp": now() + 20_000 }),
        &Ok(()),
    );
}

#[test]
fn a_not_before_beyond_the_readable_range_is_refused_by_both_postures() {
    // The row the adversarial review found, and the dangerous direction: this
    // is unreadable to `jsonwebtoken`, `nbf` is not a claim it requires, and an
    // unreadable claim it does not require constrained nothing — so the posture
    // that exists to check *more* honoured a token the canonical one refused.
    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": 1e30, "exp": now() + 20_000 }),
        &Err(IdentityError::TokenNotYetValid),
    );
}

#[test]
fn a_not_before_at_exactly_two_to_the_sixty_fourth_is_refused_by_both_postures() {
    // The exact edge of the library's readable range: its check is
    // `value < u64::MAX as f64`, and `u64::MAX as f64` is 2^64, so this is the
    // first value it rejects and therefore stops constraining.
    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": 18_446_744_073_709_551_616.0_f64, "exp": now() + 20_000 }),
        &Err(IdentityError::TokenNotYetValid),
    );
}

#[test]
fn the_largest_whole_second_not_before_is_refused_by_both_postures() {
    // One below the previous case and on the integer path, where the library
    // reads the value fine. Included so the two sides of that boundary are both
    // pinned rather than only the failing one.
    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": u64::MAX, "exp": now() + 20_000 }),
        &Err(IdentityError::TokenNotYetValid),
    );
}

#[test]
fn a_not_before_before_the_epoch_has_already_arrived_in_both_postures() {
    // The guard against over-correcting. This is also unreadable to the
    // library, but it means "valid since 1969" and must stay accepted; a fix
    // that refused every unreadable `nbf` would pass the tests above and break
    // this one.
    assert_both_postures(
        &json!({ "tenant_id": "acme", "nbf": -1, "exp": now() + 20_000 }),
        &Ok(()),
    );
}

#[test]
fn a_token_with_no_expiry_is_accepted_canonically_and_refused_in_depth() {
    // A pre-existing, deliberate divergence: `baseline` requires `exp` and the
    // canonical posture does not. `validation_rules` records the reasoning. A
    // bearer token with no `exp` never expires, and the stricter posture is the
    // one a deployment opts into to have such a token refused.
    assert_defence_in_depth_is_stricter(
        &json!({ "tenant_id": "acme" }),
        &Ok(()),
        &Err(IdentityError::UnverifiedToken),
    );
}

#[test]
fn an_expiry_beyond_the_readable_range_is_refused_only_in_depth() {
    // Same mechanism as the missing claim: the library cannot parse this, and
    // a claim it failed to parse is indistinguishable from one never sent, so
    // its required-claim check fires. The canonical posture clamps to the end
    // of representable time instead, which has not passed.
    assert_defence_in_depth_is_stricter(
        &json!({ "tenant_id": "acme", "exp": 1e30 }),
        &Ok(()),
        &Err(IdentityError::UnverifiedToken),
    );
}

#[test]
fn an_expiry_before_the_epoch_is_refused_by_both_postures_for_different_stated_reasons() {
    // Both refuse it, so the rule holds; only the reported error differs. The
    // canonical posture clamps to zero and can say "expired", while the library
    // discarded the value and can only say the required claim was missing,
    // which `classify` collapses into the opaque rejection.
    assert_defence_in_depth_is_stricter(
        &json!({ "tenant_id": "acme", "exp": -1 }),
        &Err(IdentityError::ExpiredToken),
        &Err(IdentityError::UnverifiedToken),
    );
}

#[test]
fn the_two_postures_start_from_the_same_leeway() {
    // Both windows are built from `LeewaySeconds::DEFAULT`, so a change to one
    // cannot silently widen only the other.
    assert_eq!(
        validation_rules::baseline().leeway,
        LeewaySeconds::DEFAULT.seconds()
    );
}
