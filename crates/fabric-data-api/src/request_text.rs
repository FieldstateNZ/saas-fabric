//! Text a caller supplies that no backend can store.

use serde_json::Value;

use crate::DataApiError;

/// Refuses a NUL character anywhere in caller-supplied text.
///
/// PostgreSQL cannot hold U+0000 in a text value and answers with an error the
/// connector reports as an internal failure. That would turn a caller's
/// malformed key, filter, or field into a 500 — a platform fault as far as
/// operators and retry policies are concerned — so it is refused here as the
/// 400 it is, before any connector is reached.
///
/// # Errors
///
/// [`DataApiError::BadRequest`] if `text` contains U+0000.
pub(crate) fn refuse_nul(text: &str) -> Result<(), DataApiError> {
    if text.contains('\0') {
        return Err(DataApiError::BadRequest(
            "text must not contain the NUL character".to_owned(),
        ));
    }

    Ok(())
}

/// [`refuse_nul`] over every string in a JSON value, object keys included.
///
/// # Errors
///
/// [`DataApiError::BadRequest`] if any string in `value` contains U+0000.
pub(crate) fn refuse_nul_in_json(value: &Value) -> Result<(), DataApiError> {
    match value {
        Value::String(text) => refuse_nul(text),
        Value::Array(items) => items.iter().try_for_each(refuse_nul_in_json),
        Value::Object(fields) => fields.iter().try_for_each(|(name, field)| {
            refuse_nul(name)?;
            refuse_nul_in_json(field)
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_without_nul_is_accepted() {
        assert!(refuse_nul("Alice").is_ok());
    }

    #[test]
    fn text_with_nul_is_refused() {
        assert!(matches!(refuse_nul("a\0b"), Err(DataApiError::BadRequest(_))));
    }

    #[test]
    fn nul_nested_in_a_value_or_a_field_name_is_refused() {
        for value in [
            serde_json::json!({"name": "a\u{0}b"}),
            serde_json::json!([{"tags": ["ok", "a\u{0}b"]}]),
            serde_json::json!({"na\u{0}me": 1}),
        ] {
            assert!(refuse_nul_in_json(&value).is_err(), "{value}");
        }
    }

    #[test]
    fn ordinary_json_is_accepted() {
        assert!(refuse_nul_in_json(&serde_json::json!({"a": ["b", 1, null, true]})).is_ok());
    }
}
