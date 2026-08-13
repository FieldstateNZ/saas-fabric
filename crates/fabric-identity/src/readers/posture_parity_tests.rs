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
//! it into a verdict. The defence-in-depth side is therefore driven through the
//! real [`validation_rules::baseline`] rules and the real
//! [`rejection::classify`] mapping — the two pieces of `ValidatingReader` that
//! decide *this* question — with only signature verification switched off.
//!
//! Signature verification is a separate axis with its own tests, and reaching
//! it here would mean committing an RSA private key to sign fixtures with. A
//! parity test that restated either the rules or the error mapping would be
//! worthless, since it would keep passing while the real ones drifted; sharing
//! them is the point.
//!
//! # Why the clock is real
//!
//! `jsonwebtoken` reads the system clock directly and offers no seam, so every
//! instant here is expressed relative to real "now" and the canonical reader is
//! given a clock frozen at that same second. The offsets are hours wide, so a
//! test that straddles a second boundary still lands on the same verdict.

use std::sync::Arc;

use fabric_core::{Clock, SystemClock};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header};
use serde_json::{json, Value};

use crate::readers::{rejection, validation_rules, LeewaySeconds, TrustedIngressReader};
use crate::{IdentityError, TokenReader};

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
        &EncodingKey::from_secret(b"parity-fixture"),
    )
    .unwrap()
}

/// The canonical trusted-ingress verdict on a token.
fn trusted_ingress_verdict(token: &str, at: u64) -> Result<(), IdentityError> {
    TrustedIngressReader::new(Arc::new(FrozenClock(at)))
        .read(token)
        .map(|_| ())
}

/// The defence-in-depth verdict, through the production rules and mapping.
fn validating_verdict(token: &str) -> Result<(), IdentityError> {
    let mut rules = validation_rules::baseline();

    // The one deviation from production, and the reason is above: this test is
    // about `NumericDate`s, not signatures.
    rules.insecure_disable_signature_validation();

    decode::<Value>(token, &DecodingKey::from_secret(b"parity-fixture"), &rules)
        .map(|_| ())
        .map_err(|error| rejection::classify(&error))
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
    let defence_in_depth = validating_verdict(&token);

    assert_eq!(
        canonical, defence_in_depth,
        "the postures disagreed about {claims}: trusted-ingress said {canonical:?}, \
         defence-in-depth said {defence_in_depth:?}"
    );
    assert_eq!(&canonical, expected, "both postures were wrong about {claims}");
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
fn the_two_postures_start_from_the_same_leeway() {
    // Both windows are built from `LeewaySeconds::DEFAULT`, so a change to one
    // cannot silently widen only the other.
    assert_eq!(
        validation_rules::baseline().leeway,
        LeewaySeconds::DEFAULT.seconds()
    );
}
