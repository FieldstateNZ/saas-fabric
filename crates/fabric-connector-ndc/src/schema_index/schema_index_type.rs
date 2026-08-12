//! The indexed schema itself.

use std::collections::BTreeSet;

use fabric_connector::{CollectionName, ComparisonOperator, ConnectorSchema, FieldName};

use crate::schema_index::{collection_index, operator_index, SemanticOperator};
use crate::wire::NdcSchemaResponse;

/// A connector's schema, indexed for the two lookups the request path needs:
/// which scalar type a field has, and what this connector calls a given
/// semantic operator on that type.
///
/// # Why this indirection exists
///
/// Connectors name their own operators. `ndc-postgres` calls equality `_eq`;
/// another might call it `eq` or `equals`. Hardcoding one spelling would make
/// the platform work with exactly one connector, which would defeat the point
/// of adopting a protocol at all.
///
/// The `/schema` response declares, per scalar type, what each operator name
/// *means*. Reading that at startup gives a mapping from platform semantics to
/// connector spelling, and an operator the connector never declares is refused
/// rather than guessed.
pub struct SchemaIndex {
    collections: collection_index::CollectionIndex,
    operators: operator_index::OperatorIndex,
    procedures: BTreeSet<String>,
    neutral: ConnectorSchema,
    supported: BTreeSet<ComparisonOperator>,
}

impl SchemaIndex {
    /// Indexes a connector's schema response.
    pub(crate) fn build(schema: &NdcSchemaResponse) -> Self {
        let operators = operator_index::build(schema);
        let collections = collection_index::build(schema);

        Self {
            neutral: collection_index::to_neutral_schema(&collections),
            supported: operator_index::supported_neutral_operators(&operators),
            procedures: schema.procedures.iter().map(|p| p.name.clone()).collect(),
            collections,
            operators,
        }
    }

    /// The neutral schema for the rest of the platform.
    #[must_use]
    pub const fn neutral(&self) -> &ConnectorSchema {
        &self.neutral
    }

    /// The neutral operators this connector can express somewhere.
    #[must_use]
    pub const fn supported_operators(&self) -> &BTreeSet<ComparisonOperator> {
        &self.supported
    }

    /// Whether the connector exposes a procedure by this name.
    #[must_use]
    pub fn has_procedure(&self, name: &str) -> bool {
        self.procedures.contains(name)
    }

    /// This connector's name for a semantic operator on a given field.
    ///
    /// Returns `None` when the collection or field is unknown, or when the
    /// field's scalar type declares no operator with that meaning.
    #[must_use]
    pub fn operator_name(
        &self,
        collection: &CollectionName,
        field: &FieldName,
        semantic: SemanticOperator,
    ) -> Option<&str> {
        let scalar = self.collections.get(collection.as_str())?.get(field.as_str())?;

        self.operators.get(scalar)?.get(&semantic).map(String::as_str)
    }
}
