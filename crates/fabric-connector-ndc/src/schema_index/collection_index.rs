//! Mapping collections and their fields onto scalar types.

use std::collections::BTreeMap;

use fabric_connector::{CollectionName, CollectionSchema, ConnectorSchema, FieldName};

use crate::wire::NdcSchemaResponse;

/// Collection name → field name → scalar type name.
pub(super) type CollectionIndex = BTreeMap<String, BTreeMap<String, String>>;

/// Builds the collection index from a connector's schema.
///
/// Fields whose type does not resolve to a named type — predicate-typed
/// arguments, for instance — are skipped: they cannot be compared, so including
/// them would only produce confusing lookups later.
pub(super) fn build(schema: &NdcSchemaResponse) -> CollectionIndex {
    schema
        .collections
        .iter()
        .filter_map(|collection| {
            let object_type = schema.object_types.get(&collection.collection_type)?;

            let fields = object_type
                .fields
                .iter()
                .filter_map(|(field_name, field)| {
                    field
                        .field_type
                        .named()
                        .map(|scalar| (field_name.clone(), scalar.to_owned()))
                })
                .collect();

            Some((collection.name.clone(), fields))
        })
        .collect()
}

/// Converts the index into the neutral schema the platform sees.
///
/// Names that fail platform validation are dropped rather than failing the
/// whole schema: a connector exposing one oddly-named internal table should not
/// stop the platform serving the rest. The dropped collection is then simply
/// unknown, which fails closed if a catalogue points at it.
pub(super) fn to_neutral_schema(index: &CollectionIndex) -> ConnectorSchema {
    ConnectorSchema::new(index.iter().filter_map(|(collection_name, fields)| {
        let name = CollectionName::try_new(collection_name).ok()?;
        let field_names = fields.keys().filter_map(|field| FieldName::try_new(field).ok());

        Some((name, CollectionSchema::new(field_names)))
    }))
}
