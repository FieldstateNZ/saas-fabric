//! Tests for the trusted-header operator posture.

use http::{HeaderMap, HeaderValue};

use crate::operator::{OperatorAuthError, OperatorAuthenticator, TrustedHeaderOperators};

const HEADER: &str = "Tailscale-User-Login";

fn authenticator() -> TrustedHeaderOperators {
    TrustedHeaderOperators::new(HEADER, &["brett@example.com".to_owned()]).unwrap()
}

fn headers(subject: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(subject) {
        headers.insert(HEADER, value);
    }
    headers
}

#[test]
fn an_allowlisted_subject_becomes_an_operator() {
    let operator = authenticator()
        .authenticate(&headers("brett@example.com"))
        .unwrap();

    assert_eq!(operator.subject(), "brett@example.com");
}

#[test]
fn a_request_with_no_header_is_refused() {
    assert_eq!(
        authenticator().authenticate(&HeaderMap::new()),
        Err(OperatorAuthError::Missing)
    );
}

#[test]
fn an_empty_header_is_missing_rather_than_an_empty_operator() {
    assert_eq!(
        authenticator().authenticate(&headers("   ")),
        Err(OperatorAuthError::Missing)
    );
}

#[test]
fn a_subject_outside_the_allowlist_is_refused() {
    assert_eq!(
        authenticator().authenticate(&headers("someone@example.com")),
        Err(OperatorAuthError::NotAnOperator)
    );
}

#[test]
fn case_is_folded_when_comparing_against_the_allowlist() {
    let operator = authenticator()
        .authenticate(&headers("Brett@Example.com"))
        .unwrap();

    // Folded for the comparison, preserved for the audit record.
    assert_eq!(operator.subject(), "Brett@Example.com");
}

#[test]
fn an_empty_allowlist_is_refused_at_construction() {
    // The configuration mistake this closes: an empty list read as "no
    // restriction", which on an operator network is everybody.
    assert!(TrustedHeaderOperators::new(HEADER, &[]).is_err());
    assert!(TrustedHeaderOperators::new(HEADER, &["  ".to_owned()]).is_err());
}

#[test]
fn the_description_does_not_name_the_operators() {
    // It reaches a startup log line; a list of who may administer the platform
    // does not belong in one.
    let description = authenticator().describe();

    assert!(!description.contains("brett@example.com"));
    // Lowercased, because `HeaderName` normalises: HTTP header names are
    // case-insensitive and the type stores the canonical form.
    assert!(description.contains(&HEADER.to_lowercase()));
}
