//! Turning caller JSON into neutral rows and predicates.

use fabric_connector::{ComparisonOperator, FieldName, Filter, Row};
use serde_json::{Map, Value};

use crate::{DataApiError, ResourceDefinition};

/// Builds the predicate selecting one record by its key.
///
/// The key arrives from the URL path, so it is caller-controlled — but it goes
/// into a `Filter` as a *value*, never into an identifier position, so it is
/// carried as a string and parameterised by the connector.
pub(super) fn key_filter(resource: &ResourceDefinition, key: &str) -> Filter {
    Filter::Compare {
        field: resource.key_field.clone(),
        operator: ComparisonOperator::Equal,
        value: Value::String(key.to_owned()),
    }
}

/// Converts a JSON object into a neutral row, validating every field name.
///
/// The same `permits_field` check the query parser applies, for the same
/// reason: a caller must not be able to write to a column the resource does not
/// expose, any more than it can filter on one.
///
/// # Errors
///
/// [`DataApiError::BadRequest`] for a name that is not a valid identifier, or a
/// field the resource does not expose.
pub(super) fn to_row(
    object: &Map<String, Value>,
    resource: &ResourceDefinition,
) -> Result<Row, DataApiError> {
    let mut row = Row::new();

    for (name, value) in object {
        let field = FieldName::try_new(name)
            .map_err(|error| DataApiError::BadRequest(format!("invalid field name: {error}")))?;

        if !resource.permits_field(&field) {
            return Err(DataApiError::BadRequest(format!("unknown field {field}")));
        }

        row = row.with(field, value.clone());
    }

    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(json: &str) -> ResourceDefinition {
        serde_json::from_str(json).unwrap()
    }

    fn open() -> ResourceDefinition {
        resource(r#"{"data_source":"primary","collection":"customers"}"#)
    }

    fn restricted() -> ResourceDefinition {
        resource(r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#)
    }

    #[test]
    fn the_key_filter_compares_the_configured_key_field() {
        let Filter::Compare { field, value, .. } = key_filter(&open(), "42") else {
            panic!("expected a comparison");
        };

        assert_eq!(field.as_str(), "id");
        assert_eq!(value, Value::String("42".to_owned()));
    }

    #[test]
    fn a_valid_object_becomes_a_row() {
        let object = serde_json::from_str(r#"{"name":"Alice"}"#).unwrap();

        assert_eq!(to_row(&object, &open()).unwrap().len(), 1);
    }

    #[test]
    fn a_field_the_resource_hides_cannot_be_written() {
        let object = serde_json::from_str(r#"{"salary":100000}"#).unwrap();

        assert!(to_row(&object, &restricted()).is_err());
    }

    #[test]
    fn a_field_name_that_is_not_an_identifier_is_rejected() {
        let object = serde_json::from_str(r#"{"drop table":1}"#).unwrap();

        assert!(to_row(&object, &open()).is_err());
    }
}
