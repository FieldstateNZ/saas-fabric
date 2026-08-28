//! Proving a secret's *value* cannot reach a rendered error, even through a
//! chain of `source()` calls.
//!
//! No [`ConnectorError`] variant holds a [`ResolvedSecret`] — only
//! [`SecretRef`]-shaped paths, which are safe to log (§29). These tests pin
//! that as observable behaviour rather than leaving it as something only
//! visible by reading the enum definition.

use std::fmt::Write as _;

use crate::{ConnectorError, ConnectorId};

#[test]
fn secret_unavailable_names_the_reference_but_could_never_carry_a_resolved_value() {
    // The field is a `String` path, not a `ResolvedSecret` — there is no
    // constructor for this variant that could embed a resolved value.
    let error = ConnectorError::SecretUnavailable {
        reference: "tenant/acme/data-primary".to_owned(),
    };

    let rendered = format!("{error}");

    assert!(rendered.contains("tenant/acme/data-primary"));
}

#[test]
fn the_rendered_error_chain_for_a_secret_adjacent_failure_never_contains_a_secret_value() {
    // A realistic chain: resolving a secret failed, and that surfaces to the
    // caller wrapped as "connector unreachable". The value that failed to
    // resolve must not appear anywhere in the chain — only the reference.
    const SECRET_VALUE_THAT_MUST_NEVER_APPEAR: &str = "postgres://user:hunter2@db/acme";

    let inner = ConnectorError::SecretUnavailable {
        reference: "tenant/acme/data-primary".to_owned(),
    };
    let outer = ConnectorError::Unreachable {
        connector: ConnectorId::try_new("postgres").expect("valid connector id"),
        source: Box::new(inner),
    };

    let mut rendered = format!("{outer}");
    let mut cause = std::error::Error::source(&outer);
    while let Some(error) = cause {
        let _ = write!(rendered, " -> {error}");
        cause = error.source();
    }

    assert!(!rendered.contains(SECRET_VALUE_THAT_MUST_NEVER_APPEAR));
    assert!(!rendered.contains("hunter2"));
    // The reference is safe and expected to appear — it is a path (§29).
    assert!(rendered.contains("tenant/acme/data-primary"));
}
