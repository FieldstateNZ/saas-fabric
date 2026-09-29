//! What one value must satisfy against the field it is for.
use super::{is_hostname, is_identifier, text};
use crate::errors::{invalid, ContractError};
use crate::{ConfigurationField, FieldKind};

/// Whether `value` is shaped like an IANA timezone name: `UTC`, or an
/// `Area/Location`-style pair of ASCII segments.
///
/// Checked wherever an operator submits a timezone — a custom field of kind
/// [`Timezone`](FieldKind::Timezone), a client's own timezone, the
/// platform's default region timezone — because those used to check
/// different things: the field kind checked this rule, and the others only
/// checked that *some* text had been submitted.
///
/// No segment may be empty, `.` or `..`. The character rule already excludes
/// a literal `.`, which makes both refusals unreachable today — they stay
/// explicit anyway, because a value this permits and a filesystem path some
/// future caller builds from it must never be free to disagree.
#[must_use]
pub fn is_timezone(value: &str) -> bool {
    if value == "UTC" {
        return true;
    }
    if !value.contains('/') {
        return false;
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c))
    {
        return false;
    }
    value
        .split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}
/// Checks one value against the field it is for: its text bounds, then its
/// kind. An empty value for an optional field is always legal.
///
/// # Errors
///
/// Returns [`ContractError::Invalid`] naming the field's label.
pub fn check_value(field: &ConfigurationField, value: &str) -> Result<(), ContractError> {
    text(value, &field.label, field.required, 4096)?;
    if value.is_empty() && !field.required {
        return Ok(());
    }
    let valid = match field.kind {
        FieldKind::Text => true,
        FieldKind::Number => value.parse::<f64>().is_ok_and(f64::is_finite),
        FieldKind::Boolean => matches!(value, "true" | "false"),
        FieldKind::Choice => field.options.iter().any(|option| option == value),
        FieldKind::Hostname => is_hostname(value),
        FieldKind::Identifier => is_identifier(value),
        FieldKind::Timezone => is_timezone(value),
    };
    if !valid {
        return Err(invalid(format!(
            "{} has an invalid value for its type",
            field.label
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: FieldKind, options: &[&str]) -> ConfigurationField {
        ConfigurationField {
            key: "seats".into(),
            label: "Seats".into(),
            kind,
            required: false,
            default: None,
            options: options.iter().map(|option| (*option).to_owned()).collect(),
            description: String::new(),
        }
    }

    #[test]
    fn a_dot_segment_is_not_a_timezone() {
        assert!(!is_timezone("Area/."));
        assert!(!is_timezone("Area/../etc"));
        assert!(is_timezone("UTC"));
        assert!(is_timezone("Pacific/Auckland"));
        assert!(!is_timezone("Somewhere"));
    }

    #[test]
    fn each_kind_checks_its_own_values() {
        assert!(check_value(&field(FieldKind::Number, &[]), "1.5").is_ok());
        assert!(check_value(&field(FieldKind::Number, &[]), "inf").is_err());
        assert!(check_value(&field(FieldKind::Boolean, &[]), "yes").is_err());
        assert!(check_value(&field(FieldKind::Choice, &["a"]), "a").is_ok());
        assert!(check_value(&field(FieldKind::Hostname, &[]), "a.example.com").is_ok());
        assert!(check_value(&field(FieldKind::Identifier, &[]), "Acme").is_err());
        assert!(check_value(&field(FieldKind::Timezone, &[]), "").is_ok());
        let error = check_value(&field(FieldKind::Boolean, &[]), "yes").unwrap_err();
        assert_eq!(error.to_string(), "Seats has an invalid value for its type");
    }
}
