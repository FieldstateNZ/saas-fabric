//! What [`SecretRef::distinctness_key`] treats as possibly-one-credential.
//!
//! The tests are written as pairs — two references, and whether the key says
//! they may be the same credential — because that is the only question the key
//! answers. A test asserting a particular key *string* would pin the mapping's
//! current shape rather than the property, and the mapping is allowed to get
//! coarser (never finer) without these tests having to move.

use crate::SecretRef;

/// True when the two references must be treated as possibly one credential.
fn may_be_one_credential(left: &str, right: &str) -> bool {
    SecretRef::new(left).distinctness_key() == SecretRef::new(right).distinctness_key()
}

#[test]
fn a_hyphen_and_an_underscore_are_not_a_distinction() {
    // The reported leak, at its smallest. `EnvSecretResolver` maps both of
    // these to FABRIC_SECRET_VAULT_PROD_CUSTOMER_DB_01, so they are one
    // connection string — and the runtime used to count them as two
    // destinations and let two tenants share the database unfiltered.
    assert!(may_be_one_credential(
        "vault/prod/customer-db-01",
        "vault/prod/customer_db_01"
    ));
}

#[test]
fn the_two_references_are_still_different_values() {
    // The key is a judgement about a pair, not a normalisation applied to the
    // type. Equality, ordering, hashing and resolution all still see two
    // distinct references — which is why resolution keeps working and only the
    // isolation decision changes.
    let acme = SecretRef::new("vault/prod/customer-db-01");
    let globex = SecretRef::new("vault/prod/customer_db_01");

    assert_ne!(acme, globex);
    assert_eq!(acme.as_str(), "vault/prod/customer-db-01");
    assert_eq!(globex.to_string(), "vault/prod/customer_db_01");
}

#[test]
fn a_separator_is_not_a_distinction_either() {
    // `a/b` and `a-b` flatten together too. The documented example on
    // `EnvSecretResolver` says so explicitly, and it must be caught for the
    // same reason the hyphen case is.
    assert!(may_be_one_credential("tenant/acme", "tenant-acme"));
}

#[test]
fn letter_case_is_not_a_distinction() {
    // The projection upper-cases, so two references differing only in case are
    // one variable. Two secret paths differing only in case is also exactly
    // the sort of thing a reconciler produces and a reviewer's eye skips.
    assert!(may_be_one_credential(
        "tenant/Acme/data-primary",
        "tenant/acme/DATA-primary"
    ));
}

#[test]
fn non_ascii_characters_do_not_smuggle_a_distinction_past_the_key() {
    // Every non-alphanumeric maps to one `_`, so two different non-ASCII
    // characters in the same position are not a distinction. Being wrong in
    // this direction would be a hole; being conservative merely refuses a
    // configuration nobody sensible writes.
    assert!(may_be_one_credential("tenant/café", "tenant/cafü"));
}

#[test]
fn references_that_differ_by_more_than_punctuation_stay_distinct() {
    // The other half of the rule. Over-approximating collision costs real
    // deployments their structural isolation, so the key must not fold
    // together references that name genuinely different things.
    assert!(!may_be_one_credential(
        "tenant/acme/data-primary",
        "tenant/globex/data-primary"
    ));
    assert!(!may_be_one_credential(
        "vault/prod/customer-db-01",
        "vault/prod/customer-db-02"
    ));
}

#[test]
fn length_is_a_distinction() {
    // A doubled separator is not the same as a single one. The key flattens
    // each character to one character; it does not collapse runs, so it does
    // not refuse a pair that no shipped resolver would collide.
    assert!(!may_be_one_credential("tenant//acme", "tenant/acme"));
}

#[test]
fn the_key_is_stable_for_one_reference() {
    let reference = SecretRef::new("tenant/acme/data-primary");

    assert_eq!(reference.distinctness_key(), reference.distinctness_key());
}

#[test]
fn every_reference_the_workspace_ships_keeps_its_own_key() {
    // A regression guard on over-approximation: the example configuration and
    // this crate's own documentation must not start colliding with each other.
    let shipped = [
        "tenant/initech/data-primary",
        "tenant/acme/data-primary",
        "tenant/globex/data-primary",
        "vault/tenants/acme",
    ];

    for (index, left) in shipped.iter().enumerate() {
        for right in shipped.iter().skip(index + 1) {
            assert!(
                !may_be_one_credential(left, right),
                "{left} and {right} must stay distinct"
            );
        }
    }
}
