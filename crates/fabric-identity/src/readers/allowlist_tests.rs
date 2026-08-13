//! Holds the issuer and audience allowlists to what they claim to enforce.
//!
//! # Why this file exists
//!
//! `validation_rules` asserted in prose that "`iss` and `aud` become required
//! only when a deployment configures them". The second half was true and the
//! first was not: `set_issuer` records the accepted values without touching
//! `required_spec_claims`, and `jsonwebtoken`'s `validate` skips the comparison
//! when the claim is absent, so a configured allowlist was bypassed by any
//! token that simply left the claim out. An allowlist nothing can fail is
//! worse than no allowlist, because the deployment believes it has one.
//!
//! The companion hole ran the other way. `validate_aud` defaults on, and its
//! "claim present, no allowlist" arm rejects — so enabling this posture without
//! listing audiences refused every ordinary OIDC access token.
//!
//! # Why the matrix is driven through the reader
//!
//! `allowlists` unit-tests the `Validation` fields, and those tests are
//! necessary and not sufficient — field-level tests are exactly what let the
//! original hole survive, because the fields were set correctly and the library
//! ignored them. So every row here goes through the real
//! [`ValidatingReader`](crate::ValidatingReader) and asserts the verdict a
//! caller would actually receive.
//!
//! # The one deviation
//!
//! Signature verification is switched off, for the reason `posture_parity_tests`
//! gives: signing fixtures would mean committing an RSA private key. It does not
//! weaken these rows. `decoding.rs` calls `validate` — where every claim check
//! under test here lives — regardless of that flag.

use std::sync::Arc;

use fabric_core::{Clock, SystemClock};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};

use crate::readers::{validating, ValidatingReader};
use crate::{IdentityError, TokenReader};

/// The secret both the fixture signer and the reader are given.
const FIXTURE_SECRET: &[u8] = b"allowlist-fixture";

/// The one issuer these tests configure as acceptable.
const TRUSTED_ISSUER: &str = "https://trusted.example";

/// The one audience these tests configure as acceptable.
const TRUSTED_AUDIENCE: &str = "saas-fabric";

/// The opaque rejection every allowlist failure collapses into.
///
/// `rejection::classify` deliberately refuses to distinguish a bad issuer from
/// a bad audience from a missing claim, so these rows can only assert that the
/// token was refused — which is the point.
const REFUSED: Result<(), IdentityError> = Err(IdentityError::UnverifiedToken);

/// An expiry comfortably in the future, so no row here turns on the window.
fn unexpired() -> u64 {
    SystemClock.now_unix_seconds() + 20_000
}

fn issuers() -> Vec<String> {
    vec![TRUSTED_ISSUER.to_owned()]
}

fn audiences() -> Vec<String> {
    vec![TRUSTED_AUDIENCE.to_owned()]
}

/// A reader with no allowlist configured — the default defence-in-depth
/// posture.
fn reader() -> ValidatingReader {
    validating::tests::insecure_reader(FIXTURE_SECRET, Arc::new(SystemClock))
}

/// The verdict a caller presenting this claim set would receive.
fn verdict(reader: &ValidatingReader, claims: &Value) -> Result<(), IdentityError> {
    let token = encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(FIXTURE_SECRET),
    )
    .unwrap();

    reader.read(&token).map(|_| ())
}

#[test]
fn a_matching_issuer_is_accepted() {
    let reader = reader().with_issuers(&issuers());

    assert_eq!(
        verdict(
            &reader,
            &json!({ "tenant_id": "acme", "iss": TRUSTED_ISSUER, "exp": unexpired() }),
        ),
        Ok(())
    );
}

#[test]
fn an_issuer_outside_the_allowlist_is_refused() {
    let reader = reader().with_issuers(&issuers());

    assert_eq!(
        verdict(
            &reader,
            &json!({ "tenant_id": "acme", "iss": "https://attacker.example", "exp": unexpired() }),
        ),
        REFUSED
    );
}

#[test]
fn a_token_with_no_issuer_is_refused_once_an_allowlist_is_configured() {
    // The bypass. Before the fix this returned `Ok`: the allowlist was set, the
    // claim was absent, and the library had nothing to compare.
    let reader = reader().with_issuers(&issuers());

    assert_eq!(
        verdict(&reader, &json!({ "tenant_id": "acme", "exp": unexpired() })),
        REFUSED
    );
}

#[test]
fn a_token_with_no_issuer_is_accepted_while_no_allowlist_is_configured() {
    // The guard against over-correcting: `iss` is required *because* a
    // deployment asked for it, not always. Most tokens the canonical posture
    // accepts carry no issuer this runtime knows anything about.
    assert_eq!(
        verdict(&reader(), &json!({ "tenant_id": "acme", "exp": unexpired() })),
        Ok(())
    );
}

#[test]
fn any_issuer_is_accepted_while_no_allowlist_is_configured() {
    assert_eq!(
        verdict(
            &reader(),
            &json!({ "tenant_id": "acme", "iss": "https://anyone.example", "exp": unexpired() }),
        ),
        Ok(())
    );
}

#[test]
fn a_matching_audience_is_accepted() {
    let reader = reader().with_audiences(&audiences());

    assert_eq!(
        verdict(
            &reader,
            &json!({ "tenant_id": "acme", "aud": TRUSTED_AUDIENCE, "exp": unexpired() }),
        ),
        Ok(())
    );
}

#[test]
fn an_audience_outside_the_allowlist_is_refused() {
    // Also the third-state guard: `baseline` switches `validate_aud` off, so a
    // `with_audiences` that forgot to switch it back on would accept this and
    // leave the configured allowlist doing nothing.
    let reader = reader().with_audiences(&audiences());

    assert_eq!(
        verdict(
            &reader,
            &json!({ "tenant_id": "acme", "aud": "some-other-api", "exp": unexpired() }),
        ),
        REFUSED
    );
}

#[test]
fn a_token_with_no_audience_is_refused_once_an_allowlist_is_configured() {
    let reader = reader().with_audiences(&audiences());

    assert_eq!(
        verdict(&reader, &json!({ "tenant_id": "acme", "exp": unexpired() })),
        REFUSED
    );
}

#[test]
fn an_ordinary_token_carrying_an_audience_is_accepted_while_none_are_configured() {
    // The second hole, and the one a deployment would have hit first: real
    // OIDC access tokens carry an `aud`, and `validate_aud`'s library default
    // refused every one of them the moment this mode was switched on.
    assert_eq!(
        verdict(
            &reader(),
            &json!({ "tenant_id": "acme", "aud": TRUSTED_AUDIENCE, "exp": unexpired() }),
        ),
        Ok(())
    );
}

#[test]
fn a_multi_valued_audience_is_accepted_when_one_entry_matches() {
    // How real tokens carry `aud`, and a shape the single-string path misses.
    let reader = reader().with_audiences(&audiences());

    assert_eq!(
        verdict(
            &reader,
            &json!({
                "tenant_id": "acme",
                "aud": ["some-other-api", TRUSTED_AUDIENCE],
                "exp": unexpired(),
            }),
        ),
        Ok(())
    );
}

#[test]
fn configuring_both_allowlists_requires_both_claims_whichever_order_they_are_set_in() {
    // `set_required_spec_claims` replaces the set it is given, so the naive
    // composition loses whichever claim was required first — silently, and in
    // the direction that reopens the bypass.
    let forwards = reader().with_issuers(&issuers()).with_audiences(&audiences());
    let backwards = reader().with_audiences(&audiences()).with_issuers(&issuers());

    for reader in [&forwards, &backwards] {
        assert_eq!(
            verdict(
                reader,
                &json!({
                    "tenant_id": "acme",
                    "iss": TRUSTED_ISSUER,
                    "aud": TRUSTED_AUDIENCE,
                    "exp": unexpired(),
                }),
            ),
            Ok(())
        );
        assert_eq!(
            verdict(
                reader,
                &json!({ "tenant_id": "acme", "aud": TRUSTED_AUDIENCE, "exp": unexpired() }),
            ),
            REFUSED
        );
        assert_eq!(
            verdict(
                reader,
                &json!({ "tenant_id": "acme", "iss": TRUSTED_ISSUER, "exp": unexpired() }),
            ),
            REFUSED
        );
    }
}

#[test]
fn requiring_an_allowlisted_claim_does_not_stop_requiring_an_expiry() {
    // The other half of the replacing semantics: `exp` is required by
    // `baseline` as a deliberate divergence from the canonical posture, and
    // configuring an allowlist must not quietly give that up.
    let reader = reader().with_issuers(&issuers());

    assert_eq!(
        verdict(&reader, &json!({ "tenant_id": "acme", "iss": TRUSTED_ISSUER })),
        REFUSED
    );
}

#[test]
fn an_empty_allowlist_refuses_everything_rather_than_checking_nothing() {
    // Pinned because it is the fail-closed reading of an ambiguous argument.
    // "No allowlist" is expressed by not calling the builder — which is what
    // `fabric-api` does when its configuration lists none — not by passing an
    // empty slice here.
    let reader = reader().with_issuers(&[]);

    assert_eq!(
        verdict(
            &reader,
            &json!({ "tenant_id": "acme", "iss": TRUSTED_ISSUER, "exp": unexpired() }),
        ),
        REFUSED
    );
}
