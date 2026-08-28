//! Mapping collections and their fields onto scalar types.

use std::collections::BTreeMap;

use fabric_connector::{CollectionName, CollectionSchema, ConnectorSchema, FieldName};

use crate::wire::NdcSchemaResponse;

/// Collection name → field name → the field's scalar type, where it has one.
///
/// `None` means **selectable but not comparable**: an array column, or a
/// predicate-typed one. Keeping the entry rather than dropping it is what
/// separates the two questions the request path asks. The neutral schema is
/// built from the *keys*, so such a column stays readable; the scalar lookup
/// finds nothing, so every comparison on it is refused. Dropping the field
/// instead would have made an array column vanish from the platform's schema
/// altogether, which is a wider loss than the fix needs — reading an array is
/// fine, only filtering one is unsafe.
pub(super) type CollectionIndex = BTreeMap<String, BTreeMap<String, Option<String>>>;

/// Builds the collection index from a connector's schema.
///
/// A collection whose object type the schema never defines is skipped: there
/// are no fields to record, and the collection is then simply unknown, which
/// fails closed if a catalogue points at it.
pub(super) fn build(schema: &NdcSchemaResponse) -> CollectionIndex {
    schema
        .collections
        .iter()
        .filter_map(|collection| {
            let object_type = schema.object_types.get(&collection.collection_type)?;

            let fields = object_type
                .fields
                .iter()
                .map(|(field_name, field)| (field_name.clone(), field.field_type.named().map(str::to_owned)))
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
