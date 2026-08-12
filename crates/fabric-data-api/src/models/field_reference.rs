//! Validating a caller-supplied field name.

use fabric_connector::FieldName;

use crate::{DataApiError, ResourceDefinition};

/// Parses a field name from a request and checks the resource exposes it.
///
/// **Every caller-supplied field name goes through here** — filters, sorts, and
/// projections alike — so there is one place to look when asking "can a caller
/// reference an arbitrary column?".
///
/// The exposure check covers filters, not only projections, because filtering
/// is an information channel in its own right: a caller can learn a hidden
/// column's value by narrowing a filter until rows stop coming back.
///
/// # Errors
///
/// [`DataApiError::BadRequest`] for a name that is not a valid identifier, or a
/// field the resource does not expose.
pub(super) fn parse(raw: &str, resource: &ResourceDefinition) -> Result<FieldName, DataApiError> {
    let field = FieldName::try_new(raw.trim())
        .map_err(|error| DataApiError::BadRequest(format!("invalid field name: {error}")))?;

    if !resource.permits_field(&field) {
        return Err(DataApiError::BadRequest(format!("unknown field {field}")));
    }

    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn restricted() -> ResourceDefinition {
        serde_json::from_str(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#,
        )
        .unwrap()
    }

    #[test]
    fn accepts_an_exposed_field() {
        assert_eq!(parse("name", &restricted()).unwrap().as_str(), "name");
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(parse("  name  ", &restricted()).unwrap().as_str(), "name");
    }

    #[test]
    fn rejects_a_hidden_field() {
        assert!(parse("salary", &restricted()).is_err());
    }

    #[test]
    fn rejects_a_name_that_is_not_an_identifier() {
        assert!(parse("drop table", &restricted()).is_err());
    }
}
