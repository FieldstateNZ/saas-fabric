//! The character-set rules, and the injection they exist to stop.

use super::slug::{parse_dns_label, parse_identifier, MAX_LENGTH};
use crate::IdentifierError;

#[test]
fn dns_label_accepts_a_simple_tenant_name() {
    assert_eq!(parse_dns_label("tenant id", "acme").unwrap(), "acme");
}

#[test]
fn dns_label_accepts_interior_hyphens() {
    assert_eq!(
        parse_dns_label("tenant id", "acme-corp-au").unwrap(),
        "acme-corp-au"
    );
}

#[test]
fn dns_label_rejects_uppercase_rather_than_folding_it() {
    let error = parse_dns_label("tenant id", "Acme").unwrap_err();
    assert!(matches!(
        error,
        IdentifierError::DisallowedCharacter { character: 'A', .. }
    ));
}

#[test]
fn dns_label_rejects_a_leading_hyphen() {
    let error = parse_dns_label("tenant id", "-acme").unwrap_err();
    assert!(matches!(error, IdentifierError::BadBoundary { .. }));
}

#[test]
fn dns_label_rejects_a_trailing_hyphen() {
    let error = parse_dns_label("tenant id", "acme-").unwrap_err();
    assert!(matches!(error, IdentifierError::BadBoundary { .. }));
}

#[test]
fn dns_label_rejects_sql_metacharacters() {
    // This is the case that matters: a tenant id reaches a schema name.
    let error = parse_dns_label("tenant id", "acme\";drop").unwrap_err();
    assert!(matches!(error, IdentifierError::DisallowedCharacter { .. }));
}

#[test]
fn dns_label_rejects_an_empty_value() {
    assert!(matches!(
        parse_dns_label("tenant id", "").unwrap_err(),
        IdentifierError::Empty { .. }
    ));
}

#[test]
fn dns_label_rejects_values_over_the_length_limit() {
    let long = "a".repeat(MAX_LENGTH + 1);
    assert!(matches!(
        parse_dns_label("tenant id", &long).unwrap_err(),
        IdentifierError::TooLong { .. }
    ));
}

#[test]
fn identifier_accepts_camel_case_from_the_specification() {
    assert_eq!(
        parse_identifier("logical resource name", "auditEvents").unwrap(),
        "auditEvents"
    );
}

#[test]
fn identifier_rejects_a_leading_digit() {
    assert!(matches!(
        parse_identifier("logical resource name", "1customers").unwrap_err(),
        IdentifierError::BadBoundary { .. }
    ));
}

#[test]
fn identifier_rejects_a_path_separator() {
    assert!(matches!(
        parse_identifier("logical resource name", "customers/all").unwrap_err(),
        IdentifierError::DisallowedCharacter { character: '/', .. }
    ));
}
