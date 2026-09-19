//! A logical resource one application exposes through the Data API.

use fabric_core::{LogicalDataSourceName, LogicalResourceName, OperationKind};
use fabric_runtime_publication::{CollectionName, FieldName, ResourceDefinitionDocument};
use serde::{Deserialize, Serialize};

/// One logical resource a published application release declares (ADR 0023
/// part 3): a name, the logical data source it lives in, and the collection
/// it maps to.
///
/// # Why this is not `ResourceDefinitionDocument` itself
///
/// [`into_definition`](Self::into_definition) turns one of these into the
/// runtime publisher's own [`ResourceDefinitionDocument`], field by field,
/// rather than this type simply *being* it: [`name`](Self::name) has nowhere
/// to live on that type. It is the *key* a published catalogue stores a
/// definition under -- see
/// [`CatalogDocument`](fabric_runtime_publication::CatalogDocument) -- not
/// one of the definition's own fields, so a struct that reused the wire type
/// directly would have to carry the name beside a definition that has no
/// field for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationResource {
    /// The name callers address this resource by.
    pub name: LogicalResourceName,

    /// The logical data source this resource lives in -- a name a client's
    /// `spec.data` uses, never a [`DataSourceId`](fabric_core::DataSourceId).
    /// Placement resolves that indirection later (ADR 0023 part 2); this
    /// slice does not cross-check that the name is one any client actually
    /// declared (ADR 0023, "What this does not decide").
    pub data_source: LogicalDataSourceName,

    /// The physical collection the connector knows this resource by.
    pub collection: CollectionName,

    /// The field identifying one row, for `/{id}` routes. Most resources key
    /// on `id`.
    #[serde(default = "default_key_field")]
    pub key_field: FieldName,

    /// Which operations this resource permits. Read-only by default.
    #[serde(default = "default_operations")]
    pub operations: Vec<OperationKind>,

    /// Fields callers may filter, sort and project on. Empty means
    /// unrestricted.
    #[serde(default)]
    pub queryable_fields: Vec<FieldName>,
}

impl ApplicationResource {
    /// Converts this resource into the runtime publisher's own wire type,
    /// field by field -- no logic, because every field here already means
    /// exactly what the wire's own field means.
    #[must_use]
    pub fn into_definition(self) -> (LogicalResourceName, ResourceDefinitionDocument) {
        (
            self.name,
            ResourceDefinitionDocument {
                data_source: self.data_source,
                collection: self.collection,
                key_field: self.key_field,
                operations: self.operations,
                queryable_fields: self.queryable_fields,
            },
        )
    }
}

/// Most resources key on `id` -- the same default
/// [`ResourceDefinitionDocument`] itself falls back to.
fn default_key_field() -> FieldName {
    FieldName::try_new("id").unwrap_or_else(|_| unreachable!("\"id\" is a valid field name"))
}

/// Read-only unless the application declares otherwise -- the same default
/// [`ResourceDefinitionDocument`] itself falls back to.
fn default_operations() -> Vec<OperationKind> {
    vec![OperationKind::Read, OperationKind::List]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn customers() -> ApplicationResource {
        ApplicationResource {
            name: LogicalResourceName::try_new("customers").unwrap(),
            data_source: LogicalDataSourceName::try_new("primary").unwrap(),
            collection: CollectionName::try_new("customers").unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![OperationKind::Read, OperationKind::List],
            queryable_fields: vec![FieldName::try_new("id").unwrap()],
        }
    }

    #[test]
    fn the_wire_is_camel_case() {
        let text = serde_norway::to_string(&customers()).unwrap();

        assert!(text.contains("dataSource: primary"), "{text}");
        assert!(text.contains("keyField: id"), "{text}");
        assert!(text.contains("queryableFields:"), "{text}");
    }

    #[test]
    fn an_omitted_key_field_and_operations_take_the_same_defaults_the_wire_does() {
        let resource: ApplicationResource =
            serde_norway::from_str("name: customers\ndataSource: primary\ncollection: customers\n").unwrap();

        assert_eq!(resource.key_field.as_str(), "id");
        assert_eq!(
            resource.operations,
            vec![OperationKind::Read, OperationKind::List]
        );
        assert!(resource.queryable_fields.is_empty());
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let error = serde_norway::from_str::<ApplicationResource>(
            "name: customers\ndataSource: primary\ncollection: customers\nbogus: true\n",
        )
        .unwrap_err();

        assert!(error.to_string().contains("bogus"), "{error}");
    }

    #[test]
    fn a_resource_round_trips_into_the_wires_own_definition_and_back() {
        let resource = customers();

        let (name, definition) = resource.clone().into_definition();

        assert_eq!(name, resource.name);
        assert_eq!(definition.data_source, resource.data_source);
        assert_eq!(definition.collection, resource.collection);
        assert_eq!(definition.key_field, resource.key_field);
        assert_eq!(definition.operations, resource.operations);
        assert_eq!(definition.queryable_fields, resource.queryable_fields);
    }
}
