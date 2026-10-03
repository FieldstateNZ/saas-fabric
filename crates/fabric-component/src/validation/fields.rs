//! What a configuration schema must look like.
//!
//! A configuration field's shape and a value's legality are one contract,
//! checked in two directions: [`validate_fields`] is what a schema must look
//! like, and `values.rs` is what a value must satisfy against a field that
//! shape already produced -- including each field's default, which is why
//! this file calls it. Checking a whole submitted configuration against a
//! schema stays in `fabric-client-model`, which calls
//! [`check_value`](super::check_value) for each value.
use super::{check_value, text, unique};
use crate::errors::{invalid, ContractError};
use crate::{ConfigurationField, FieldKind};

/// Checks a configuration schema: unique keys, each key, label, description
/// and option within its bounds, options only on a choice field (and at
/// least one there), and every default a legal value for its field.
///
/// # Errors
///
/// Returns [`ContractError::Invalid`] for the first rule a field breaks.
pub fn validate_fields(fields: &[ConfigurationField]) -> Result<(), ContractError> {
    unique(fields.iter().map(|field| field.key.as_str()), "field")?;
    for field in fields {
        check_key(&field.key)?;
        text(&field.key, "Field key", true, 128)?;
        text(&field.label, "Field label", true, 128)?;
        text(&field.description, "Field description", false, 1024)?;
        unique(field.options.iter().map(String::as_str), "choice")?;
        if field.kind == FieldKind::Choice {
            if field.options.is_empty() {
                return Err(invalid("A choice field requires options"));
            }
        } else if !field.options.is_empty() {
            return Err(invalid("Only a choice field may declare options"));
        }
        for option in &field.options {
            text(option, "Choice", true, 256)?;
        }
        if let Some(default) = &field.default {
            check_value(field, default)?;
        }
    }
    Ok(())
}
/// The character rule a configuration key must follow, wherever one is
/// written: a custom field's own key, and a plan's configuration key, which
/// is checked against the same fields and so must follow the same rule to
/// ever match one.
///
/// # Errors
///
/// Returns [`ContractError::Invalid`] for an empty key or one with a
/// character other than an ASCII letter, digit, underscore or hyphen.
pub fn check_key(key: &str) -> Result<(), ContractError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(invalid(
            "Configuration keys must contain only letters, numbers, underscores and hyphens",
        ));
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
    fn options_on_a_non_choice_field_are_refused() {
        let error = validate_fields(&[field(FieldKind::Number, &["10"])]).unwrap_err();

        assert_eq!(error.to_string(), "Only a choice field may declare options");
    }

    #[test]
    fn a_key_outside_the_character_rule_is_refused() {
        let error = check_key("seats!").unwrap_err();

        assert!(error.to_string().contains("letters, numbers"), "{error}");
    }
}
