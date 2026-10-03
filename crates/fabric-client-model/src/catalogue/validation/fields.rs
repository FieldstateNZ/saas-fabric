//! Type checking for configuration schemas and submitted values.
//!
//! What a schema must look like, and what one value must satisfy against a
//! field, are `fabric-component`'s rules (ADR 0026 section 2): a component
//! descriptor declares fields in the same shape. Checking a whole submitted
//! configuration against a schema is the catalogue's alone, and stays here.
use super::invalid;
use crate::catalogue::{ConfigurationField, ConfigurationValues};
use crate::DesiredStateError;
pub(in crate::catalogue) use fabric_component::is_timezone;

/// `fabric_component::validate_fields`, as a desired-state error.
pub(in crate::catalogue) fn validate_fields(fields: &[ConfigurationField]) -> Result<(), DesiredStateError> {
    Ok(fabric_component::validate_fields(fields)?)
}
/// `fabric_component::check_key`, as a desired-state error: the character
/// rule a custom field's key and a plan's configuration key share.
pub(in crate::catalogue) fn check_key(key: &str) -> Result<(), DesiredStateError> {
    Ok(fabric_component::check_key(key)?)
}
/// `fabric_component::check_value`, as a desired-state error.
fn check(field: &ConfigurationField, value: &str) -> Result<(), DesiredStateError> {
    Ok(fabric_component::check_value(field, value)?)
}
pub(in crate::catalogue) fn values(
    fields: &[ConfigurationField],
    submitted: &ConfigurationValues,
) -> Result<ConfigurationValues, DesiredStateError> {
    if submitted
        .keys()
        .any(|key| !fields.iter().any(|field| &field.key == key))
    {
        return Err(invalid("Configuration contains an undeclared field"));
    }
    let mut resolved = ConfigurationValues::new();
    for field in fields {
        let value = submitted.get(&field.key).or(field.default.as_ref());
        if let Some(value) = value {
            check(field, value)?;
            resolved.insert(field.key.clone(), value.clone());
        } else if field.required {
            return Err(invalid(format!("{} is required", field.label)));
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::FieldKind;

    #[test]
    fn a_dot_segment_is_not_a_timezone() {
        assert!(!is_timezone("Area/."));
    }

    #[test]
    fn a_dot_dot_segment_is_not_a_timezone() {
        assert!(!is_timezone("Area/../etc"));
    }

    #[test]
    fn utc_and_an_area_location_pair_are_timezones() {
        assert!(is_timezone("UTC"));
        assert!(is_timezone("Pacific/Auckland"));
    }

    #[test]
    fn plain_text_with_no_slash_is_not_a_timezone() {
        assert!(!is_timezone("Somewhere"));
    }

    #[test]
    fn a_plan_configuration_key_follows_the_field_key_rule() {
        let mut values = ConfigurationValues::new();
        values.insert("seats!".into(), "10".into());

        let error = super::super::config(&values).unwrap_err();

        assert!(
            matches!(error, DesiredStateError::InvalidField { ref detail, .. } if detail.contains("letters, numbers")),
            "{error}"
        );
    }

    #[test]
    fn options_on_a_non_choice_field_are_refused() {
        let fields = vec![ConfigurationField {
            key: "seats".into(),
            label: "Seats".into(),
            kind: FieldKind::Number,
            required: false,
            default: None,
            options: vec!["10".into()],
            description: String::new(),
        }];

        let error = validate_fields(&fields).unwrap_err();

        assert!(
            matches!(error, DesiredStateError::InvalidField { ref detail, .. } if detail.contains("choice field")),
            "{error}"
        );
    }
}
