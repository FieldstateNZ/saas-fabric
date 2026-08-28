//! Turning caller JSON into neutral rows and predicates.

use fabric_connector::{ComparisonOperator, FieldName, Filter, Row};
use serde_json::{Map, Value};

use crate::models::WritableFields;
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
/// Gated on [`WritableFields`] rather than on
/// [`ResourceDefinition::permits_field`] alone. That is the whole point of the
/// type: the catalogue rule is only half of what may be written, and the other
/// half — the tenant discriminator, under any casing — is a property of where
/// this tenant is *placed* and is not knowable from the catalogue at all.
///
/// Refusing a field here is what makes it un-writable, structurally. It no
/// longer depends on
/// [`MutationSpec::for_target`](fabric_connector::MutationSpec::for_target)
/// overwriting the right string afterwards, which held only while the backend
/// compared column names exactly as this crate does.
///
/// # Errors
///
/// [`DataApiError::BadRequest`] for a name that is not a valid identifier, or a
/// field this operation may not write. Both refusals read the same to a caller
/// on purpose — see [`WritableFields`] for why the discriminator is not named.
pub(super) fn to_row(
    object: &Map<String, Value>,
    writable: &WritableFields<'_>,
) -> Result<Row, DataApiError> {
    let mut row = Row::new();

    for (name, value) in object {
        let field = FieldName::try_new(name)
            .map_err(|error| DataApiError::BadRequest(format!("invalid field name: {error}")))?;

        if !writable.permits(&field) {
            return Err(DataApiError::BadRequest(format!("unknown field {field}")));
        }

        row = row.with(field, value.clone());
    }

    Ok(row)
}

#[cfg(test)]
mod tests {
    use fabric_connector::IsolationModel;

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

    fn shared() -> IsolationModel {
        IsolationModel::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "tenant-482".to_owned(),
        }
    }

    fn object(json: &str) -> Map<String, Value> {
        serde_json::from_str(json).unwrap()
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
        let open = open();
        let writable = WritableFields::new(&open, &IsolationModel::Database);

        assert_eq!(
            to_row(&object(r#"{"name":"Alice"}"#), &writable).unwrap().len(),
            1
        );
    }

    #[test]
    fn a_field_the_resource_hides_cannot_be_written() {
        let restricted = restricted();
        let writable = WritableFields::new(&restricted, &IsolationModel::Database);

        assert!(to_row(&object(r#"{"salary":100000}"#), &writable).is_err());
    }

    #[test]
    fn a_field_name_that_is_not_an_identifier_is_rejected() {
        let open = open();
        let writable = WritableFields::new(&open, &IsolationModel::Database);

        assert!(to_row(&object(r#"{"drop table":1}"#), &writable).is_err());
    }

    #[test]
    fn the_discriminator_cannot_be_written_on_a_shared_table() {
        let open = open();
        let shared = shared();
        let writable = WritableFields::new(&open, &shared);

        assert!(to_row(&object(r#"{"tenant_key":"tenant-999"}"#), &writable).is_err());
    }

    #[test]
    fn a_case_variant_of_the_discriminator_cannot_be_written_either() {
        // Previously this passed `permits_field` and rode to the connector
        // beside the correct stamp, because `FieldName` compares by exact case.
        let open = open();
        let shared = shared();
        let writable = WritableFields::new(&open, &shared);

        let error = to_row(
            &object(r#"{"name":"Mallory","TENANT_KEY":"tenant-999"}"#),
            &writable,
        )
        .expect_err("a case variant of the discriminator must be refused");

        // And it is refused in the same words as any other unwritable field:
        // §26 keeps the isolation model from the application.
        assert!(matches!(error, DataApiError::BadRequest(message) if message == "unknown field TENANT_KEY"));
    }

    #[test]
    fn the_discriminator_name_is_ordinary_on_a_dedicated_placement() {
        // There is no discriminator to protect, so nothing is refused: the rule
        // follows the placement, not the column name.
        let open = open();
        let writable = WritableFields::new(&open, &IsolationModel::Database);

        assert!(to_row(&object(r#"{"tenant_key":"anything"}"#), &writable).is_ok());
    }
}
