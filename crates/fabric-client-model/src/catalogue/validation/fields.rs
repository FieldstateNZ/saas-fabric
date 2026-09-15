//! Type checking for configuration schemas and submitted values.
use super::{invalid, text, unique};
use crate::catalogue::{ConfigurationField, ConfigurationValues, FieldKind};
use crate::{ClientId, DesiredStateError, Host};

pub(in crate::catalogue) fn validate_fields(fields: &[ConfigurationField]) -> Result<(), DesiredStateError> {
    unique(fields.iter().map(|field| field.key.as_str()), "field")?;
    for field in fields {
        if field.key.is_empty()
            || !field
                .key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(invalid(
                "Field keys must contain only letters, numbers, underscores and hyphens",
            ));
        }
        text(&field.key, "Field key", true, 128)?;
        text(&field.label, "Field label", true, 128)?;
        text(&field.description, "Field description", false, 1024)?;
        unique(field.options.iter().map(String::as_str), "choice")?;
        if field.kind == FieldKind::Choice && field.options.is_empty() {
            return Err(invalid("A choice field requires options"));
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
        FieldKind::Timezone => {
            value == "UTC"
                || (value.contains('/')
                    && value
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c)))
        }
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
