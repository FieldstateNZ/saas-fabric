//! Shared character-set checks behind the identifier newtypes.
//!
//! These are pure functions rather than a trait or a base type: the two rule
//! sets are genuinely independent, and a junior reading `TenantId::try_new`
//! should be able to follow one function call to the exact rule that applies.

use crate::IdentifierError;

/// The inclusive maximum length shared by every platform identifier.
///
/// 63 bytes is the DNS label limit, and it is also comfortably under the
/// PostgreSQL identifier limit of 63 bytes for a schema name. Picking the
/// smaller of the two bounds up front means a tenant id can always be used as
/// either without a second check.
pub const MAX_LENGTH: usize = 63;

/// Validates a strict DNS-label-style identifier: lowercase ASCII letters,
/// digits, and interior hyphens.
///
/// This is the rule for values that leave the process — tenant ids appear in
/// schema names, connection pool keys, metric labels, and log fields, so the
/// permitted set is deliberately narrow. Uppercase is rejected rather than
/// folded, because silently lowercasing would make `Acme` and `acme` the same
/// tenant at one layer and different tenants at another.
///
/// # Errors
///
/// Returns [`IdentifierError`] describing the first rule the value broke.
pub fn parse_dns_label(kind: &'static str, value: &str) -> Result<String, IdentifierError> {
    check_length(kind, value)?;

    for character in value.chars() {
        let permitted = character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-';
        if !permitted {
            return Err(IdentifierError::DisallowedCharacter {
                kind,
                character,
                expected: "lowercase ASCII letters, digits, and hyphens",
            });
        }
    }

    check_boundaries(kind, value)?;

    Ok(value.to_owned())
}

/// Validates a programmer-facing identifier: an ASCII letter followed by
/// letters, digits, hyphens, or underscores.
///
/// Logical resource and data-source names are written by platform engineers in
/// configuration rather than derived from tenant input, and the specification's
/// own examples use camelCase (`auditEvents`), so mixed case is permitted here
/// where it is not for a tenant id.
///
/// # Errors
///
/// Returns [`IdentifierError`] describing the first rule the value broke.
pub fn parse_identifier(kind: &'static str, value: &str) -> Result<String, IdentifierError> {
    check_length(kind, value)?;

    for character in value.chars() {
        let permitted = character.is_ascii_alphanumeric() || character == '-' || character == '_';
        if !permitted {
            return Err(IdentifierError::DisallowedCharacter {
                kind,
                character,
                expected: "ASCII letters, digits, hyphens, and underscores",
            });
        }
    }

    let starts_with_letter = value.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    if !starts_with_letter {
        return Err(IdentifierError::BadBoundary { kind });
    }

    Ok(value.to_owned())
}

/// Rejects empty and over-long values, which both rule sets share.
fn check_length(kind: &'static str, value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::Empty { kind });
    }

    if value.len() > MAX_LENGTH {
        return Err(IdentifierError::TooLong {
            kind,
            max: MAX_LENGTH,
            actual: value.len(),
        });
    }

    Ok(())
}

/// Rejects leading and trailing hyphens, which are legal characters but not in
/// those positions.
fn check_boundaries(kind: &'static str, value: &str) -> Result<(), IdentifierError> {
    let first_ok = value.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    let last_ok = value
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric());

    if first_ok && last_ok {
        Ok(())
    } else {
        Err(IdentifierError::BadBoundary { kind })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
