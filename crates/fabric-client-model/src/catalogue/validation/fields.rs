//! Type checking for configuration schemas and submitted values.
//!
//! In the 121–150 line band. The reason is that a configuration field's
//! shape and a submitted value's legality are one contract, checked in two
//! directions: `validate_fields` is what a schema must look like,
//! `values`/`check` is what a value must satisfy against a field that
//! shape already produced. `check_key` and `is_timezone` are the two rules
//! both directions rely on, so they stay where both can reach them rather
//! than in a file only one of the two would import.
use super::{invalid, text, unique};
use crate::catalogue::{ConfigurationField, ConfigurationValues, FieldKind};
use crate::{ClientId, DesiredStateError, Host};

pub(in crate::catalogue) fn validate_fields(fields: &[ConfigurationField]) -> Result<(), DesiredStateError> {
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
            check(field, default)?;
        }
    }
    Ok(())
}
/// The character rule a configuration key must follow, wherever one is
/// written: a custom field's own key, and a plan's configuration key, which
/// [`values`] checks against the same fields and so must follow the same
/// rule to ever match one.
pub(in crate::catalogue) fn check_key(key: &str) -> Result<(), DesiredStateError> {
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
pub(in crate::catalogue) fn is_timezone(value: &str) -> bool {
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
fn check(field: &ConfigurationField, value: &str) -> Result<(), DesiredStateError> {
    text(value, &field.label, field.required, 4096)?;
    if value.is_empty() && !field.required {
        return Ok(());
    }
    let valid = match field.kind {
        FieldKind::Text => true,
        FieldKind::Number => value.parse::<f64>().is_ok_and(f64::is_finite),
        FieldKind::Boolean => matches!(value, "true" | "false"),
        FieldKind::Choice => field.options.iter().any(|option| option == value),
        FieldKind::Hostname => Host::try_new(value).is_ok(),
        FieldKind::Identifier => ClientId::try_new(value).is_ok(),
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
