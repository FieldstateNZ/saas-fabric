//! What a secret path refuses.
//!
//! The boundary is enforced by prefixing a client's namespace. A path that can
//! climb out of that prefix makes the enforcement decorative, so the traversal
//! cases are the ones that matter.

use crate::{SecretPath, SecretValues};

#[test]
fn ordinary_paths_are_accepted() {
    for path in ["smtp", "database/primary", "third-party/stripe.live", "a_b.c-d/e"] {
        assert!(SecretPath::parse(path).is_ok(), "{path} should be a valid path");
    }
}

#[test]
fn nothing_may_climb_out_of_the_boundary() {
    for attempt in [
        "..",
        "../other",
        "database/../../other",
        "database/./primary",
        "/absolute",
        "trailing/",
        "double//segment",
    ] {
        assert!(
            SecretPath::parse(attempt).is_err(),
            "{attempt} must not be addressable"
        );
    }
}

#[test]
fn an_encoded_separator_is_refused_rather_than_decoded_later() {
    // A percent sign would let a caller smuggle a separator the store decodes
    // downstream — the same escape by a slower route.
    for attempt in ["a%2f..%2fb", "a%2Fb", "a b", "a\\b", "a#b", "a?b"] {
        assert!(SecretPath::parse(attempt).is_err(), "{attempt} must be refused");
    }
}

#[test]
fn a_secrets_debug_output_never_carries_a_value() {
    let values = SecretValues::new(
        [("password".to_owned(), "super-secret".to_owned())]
            .into_iter()
            .collect(),
    );

    let rendered = format!("{values:?}");

    assert!(
        rendered.contains("password"),
        "the key name is useful: {rendered}"
    );
    assert!(
        !rendered.contains("super-secret"),
        "a debug format must never carry a value: {rendered}"
    );
}
