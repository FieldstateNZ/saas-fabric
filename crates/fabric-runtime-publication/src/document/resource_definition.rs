//! What one logical resource is, as the publisher declares it.

use fabric_core::{LogicalDataSourceName, OperationKind};

use crate::{CollectionName, FieldName};

/// The publisher's own declaration of one catalogue entry.
///
/// Mirrors `fabric_data_api::ResourceDefinition` — see
/// [`crate::TenantBindingDocument`] for why this crate declares its own copy.
///
/// # `queryable_fields` is not optional here
///
/// The consumer defaults a missing `queryable_fields` to "unrestricted",
/// which is the right behaviour for a resource that has not opted into
/// hiding anything. But an *omission* on the wire and a deliberate, explicit
/// "no restriction" look identical once they reach that default — and the
/// runtime cannot tell an operator's considered choice from a forgotten
/// field. Making the field non-optional on the producer's own type turns a
/// forgotten field into a compile error wherever this crate *builds* a
/// catalogue, while `#[serde(default)]` still lets this type *parse* a
/// document that omitted the key, such as the shipped `examples/catalog.json`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDefinitionDocument {
    /// The logical data source this resource lives in — `primary`, `audit`.
    pub data_source: LogicalDataSourceName,

    /// The physical collection name the connector knows.
    pub collection: CollectionName,

    /// The field identifying a single row, for `/{id}` routes.
    #[serde(default = "default_key_field")]
    pub key_field: FieldName,

    /// Which operations are exposed for this resource. Read-only by default.
    #[serde(default = "default_operations")]
    pub operations: Vec<OperationKind>,

    /// Fields callers may filter, sort, and project on — and the only fields
    /// a response may carry. Always present on the wire; an empty list means
    /// unrestricted.
    #[serde(default)]
    pub queryable_fields: Vec<FieldName>,
}

/// Most collections key on `id`.
fn default_key_field() -> FieldName {
    FieldName::try_new("id").unwrap_or_else(|_| unreachable!("\"id\" is a valid field name"))
}

/// Read-only unless the catalogue says otherwise.
fn default_operations() -> Vec<OperationKind> {
    vec![OperationKind::Read, OperationKind::List]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn customers() -> ResourceDefinitionDocument {
        ResourceDefinitionDocument {
            data_source: LogicalDataSourceName::try_new("primary").unwrap(),
            collection: CollectionName::try_new("customers").unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![OperationKind::Read],
            queryable_fields: Vec::new(),
        }
    }

    #[test]
    fn queryable_fields_is_always_emitted_because_the_runtime_treats_absence_as_unrestricted() {
        let json = serde_json::to_value(customers()).unwrap();

        assert!(json.get("queryable_fields").is_some(), "{json}");
        assert_eq!(json["queryable_fields"], serde_json::json!([]));
    }

    #[test]
    fn an_absent_queryable_fields_key_still_parses_as_unrestricted() {
        let definition: ResourceDefinitionDocument =
            serde_json::from_str(r#"{"data_source":"primary","collection":"customers"}"#).unwrap();

        assert!(definition.queryable_fields.is_empty());
    }

    #[test]
    fn the_key_field_defaults_to_id() {
        let definition: ResourceDefinitionDocument =
            serde_json::from_str(r#"{"data_source":"primary","collection":"customers"}"#).unwrap();

        assert_eq!(definition.key_field.as_str(), "id");
    }
}
