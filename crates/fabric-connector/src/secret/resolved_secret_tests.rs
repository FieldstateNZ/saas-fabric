//! Proving `ResolvedSecret` cannot print its value, under increasingly
//! adversarial ways of trying to print it.
//!
//! A hand-written `Debug` on `ResolvedSecret` itself is not enough on its
//! own: `derive(Debug)` on a *containing* struct calls straight through to
//! the field's `Debug` impl, so the redaction only holds if it survives being
//! nested. Each test here puts the secret one layer further from the type
//! that actually knows to redact it.

use std::collections::HashMap;

use crate::{ResolvedSecret, SecretRef};

const VALUE: &str = "postgres://user:hunter2@db/acme";

#[test]
fn a_resolved_secret_never_prints_its_value() {
    let secret = ResolvedSecret::new(VALUE);

    let printed = format!("{secret:?}");

    assert_eq!(printed, "ResolvedSecret(<redacted>)");
    assert!(!printed.contains("hunter2"));
}

#[test]
fn the_value_is_reachable_only_through_expose() {
    let secret = ResolvedSecret::new("hunter2");
    assert_eq!(secret.expose(), "hunter2");
}

#[test]
fn a_secret_reference_is_not_itself_sensitive_and_prints_normally() {
    // Unlike `ResolvedSecret`, a `SecretRef` is a path, not a value, and is
    // safe to print — this pins that this remains a deliberate difference,
    // not an oversight shared with the type it points at.
    let reference = SecretRef::new("tenant/acme/data-primary");
    assert_eq!(format!("{reference}"), "tenant/acme/data-primary");
}

#[test]
fn a_resolved_secret_nested_in_a_deriving_struct_still_does_not_print_its_value() {
    // `derive(Debug)` on `Connection` calls `ResolvedSecret::fmt` for its
    // field — it cannot see past the type's own redaction, so nesting one
    // layer deeper changes nothing.
    #[derive(Debug)]
    struct Connection {
        #[allow(dead_code)]
        credential: ResolvedSecret,
    }

    let wrapper = Connection {
        credential: ResolvedSecret::new(VALUE),
    };

    let printed = format!("{wrapper:?}");

    assert!(!printed.contains("hunter2"));
    assert!(printed.contains("<redacted>"));
}

#[test]
fn a_resolved_secret_inside_an_option_still_does_not_print_its_value() {
    let secret: Option<ResolvedSecret> = Some(ResolvedSecret::new(VALUE));

    let printed = format!("{secret:?}");

    assert!(!printed.contains("hunter2"));
}

#[test]
fn a_resolved_secret_inside_a_vec_still_does_not_print_its_value() {
    let secrets = vec![ResolvedSecret::new(VALUE), ResolvedSecret::new("other-hunter3")];

    let printed = format!("{secrets:?}");

    assert!(!printed.contains("hunter2"));
    assert!(!printed.contains("hunter3"));
}

#[test]
fn a_resolved_secret_inside_a_map_value_still_does_not_print_its_value() {
    let mut secrets = HashMap::new();
    secrets.insert("primary", ResolvedSecret::new(VALUE));

    let printed = format!("{secrets:?}");

    assert!(!printed.contains("hunter2"));
}

#[test]
fn a_resolved_secret_inside_a_result_err_still_does_not_print_its_value() {
    #[derive(Debug)]
    #[allow(dead_code)]
    struct ConnectFailed(ResolvedSecret);

    let outcome: Result<(), ConnectFailed> = Err(ConnectFailed(ResolvedSecret::new(VALUE)));

    let printed = format!("{outcome:?}");

    assert!(!printed.contains("hunter2"));
}
