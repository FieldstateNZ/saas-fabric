//! What the connector told us it has, indexed for use.

use std::collections::{BTreeMap, BTreeSet};

use fabric_connector::{CollectionName, CollectionSchema, ComparisonOperator, ConnectorSchema, FieldName};

use crate::wire::{NdcComparisonOperatorDefinition, NdcSchemaResponse};

/// The semantic operators the platform knows how to ask for.
///
/// Note the absence of "not equal": NDC has no such semantic. Inequality is
/// expressed as a negated equality, which every connector supporting `Equal`
/// can therefore do. That is handled in translation, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticOperator {
    /// Equality — also the basis for inequality, via negation.
    Equal,
    /// Membership in a list.
    In,
    /// Strictly less than.
    LessThan,
    /// Less than or equal.
    LessThanOrEqual,
    /// Strictly greater than.
    GreaterThan,
    /// Greater than or equal.
    GreaterThanOrEqual,
    /// Substring containment.
    Contains,
}

impl SemanticOperator {
    /// The semantic a neutral operator needs from the connector.
    #[must_use]
    pub const fn for_neutral(operator: ComparisonOperator) -> Self {
        match operator {
            // Inequality is a negated equality, so it needs the same operator.
            ComparisonOperator::Equal | ComparisonOperator::NotEqual => Self::Equal,
            ComparisonOperator::LessThan => Self::LessThan,
            ComparisonOperator::LessThanOrEqual => Self::LessThanOrEqual,
            ComparisonOperator::GreaterThan => Self::GreaterThan,
            ComparisonOperator::GreaterThanOrEqual => Self::GreaterThanOrEqual,
            ComparisonOperator::Contains => Self::Contains,
        }
    }

    /// Reads the semantic out of a connector's operator definition.
    ///
    /// Case-insensitive containment is accepted as containment. It is a
    /// deliberate widening — a connector offering only the insensitive form can
    /// still serve a containment query, and returning extra matches on a
    /// *caller's* text filter is not a tenancy concern (tenant scoping is
    /// always equality on the discriminator, never containment).
    fn from_definition(definition: &NdcComparisonOperatorDefinition) -> Option<Self> {
        match definition {
            NdcComparisonOperatorDefinition::Equal => Some(Self::Equal),
            NdcComparisonOperatorDefinition::In => Some(Self::In),
            NdcComparisonOperatorDefinition::LessThan => Some(Self::LessThan),
            NdcComparisonOperatorDefinition::LessThanOrEqual => Some(Self::LessThanOrEqual),
            NdcComparisonOperatorDefinition::GreaterThan => Some(Self::GreaterThan),
            NdcComparisonOperatorDefinition::GreaterThanOrEqual => Some(Self::GreaterThanOrEqual),
            NdcComparisonOperatorDefinition::Contains
            | NdcComparisonOperatorDefinition::ContainsInsensitive => Some(Self::Contains),
            NdcComparisonOperatorDefinition::StartsWith
            | NdcComparisonOperatorDefinition::StartsWithInsensitive
            | NdcComparisonOperatorDefinition::EndsWith
            | NdcComparisonOperatorDefinition::EndsWithInsensitive
            | NdcComparisonOperatorDefinition::Custom => None,
        }
    }
}

/// A connector's schema, indexed for the two lookups the request path needs:
/// which scalar type a field has, and what this connector calls a given
/// semantic operator on that type.
///
/// # Why this indirection exists
///
/// Connectors name their own operators. `ndc-postgres` calls equality `_eq`;
/// another might call it `eq` or `equals`. Hardcoding one spelling would make
/// the platform work with exactly one connector, which would defeat the point
/// of adopting a protocol.
///
/// The `/schema` response declares, per scalar type, what each operator name
/// *means*. Reading that at startup gives a mapping from platform semantics to
/// connector spelling, and an operator the connector never declares is refused
/// rather than guessed.
pub struct SchemaIndex {
    /// Collection name to the fields it has and their scalar types.
    collections: BTreeMap<String, BTreeMap<String, String>>,

    /// Scalar type name to the operator name for each semantic it supports.
    operators: BTreeMap<String, BTreeMap<SemanticOperator, String>>,

    /// Procedure names the connector exposes.
    procedures: BTreeSet<String>,

    /// The neutral view handed to the rest of the platform.
    neutral: ConnectorSchema,

    /// Operators supported by at least one scalar type.
    supported: BTreeSet<ComparisonOperator>,
}

impl SchemaIndex {
    /// Indexes a connector's schema response.
    pub(crate) fn build(schema: &NdcSchemaResponse) -> Self {
        let operators = index_operators(schema);
        let collections = index_collections(schema);
        let neutral = build_neutral_schema(&collections);
        let supported = supported_operators(&operators);

        Self {
            collections,
            operators,
            procedures: schema.procedures.iter().map(|p| p.name.clone()).collect(),
            neutral,
            supported,
        }
    }

    /// The neutral schema for the rest of the platform.
    #[must_use]
    pub fn neutral(&self) -> &ConnectorSchema {
        &self.neutral
    }

    /// The neutral operators this connector can express somewhere.
    ///
    /// Support is genuinely per scalar type, but
    /// [`ConnectorCapabilities`](fabric_connector::ConnectorCapabilities) is a
    /// single global set. This reports the union, which is the permissive
    /// answer — the authoritative per-field check happens in
    /// [`Self::operator_name`] at translation time and fails closed there.
    #[must_use]
    pub fn supported_operators(&self) -> &BTreeSet<ComparisonOperator> {
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

/// Builds scalar type → semantic → connector operator name.
///
/// Where a scalar declares two names with the same meaning, the first in name
/// order wins. Deterministic beats arbitrary: the generated query should not
/// change between restarts.
fn index_operators(schema: &NdcSchemaResponse) -> BTreeMap<String, BTreeMap<SemanticOperator, String>> {
    schema
        .scalar_types
        .iter()
        .map(|(scalar_name, scalar)| {
            let mut by_semantic = BTreeMap::new();

            for (operator_name, definition) in &scalar.comparison_operators {
                if let Some(semantic) = SemanticOperator::from_definition(definition) {
                    by_semantic
                        .entry(semantic)
                        .or_insert_with(|| operator_name.clone());
                }
            }

            (scalar_name.clone(), by_semantic)
        })
        .collect()
}

/// Builds collection → field → scalar type name.
///
/// Fields whose type does not resolve to a named type — predicate-typed
/// arguments, for instance — are skipped: they cannot be compared, so
/// including them would only produce confusing lookups later.
fn index_collections(schema: &NdcSchemaResponse) -> BTreeMap<String, BTreeMap<String, String>> {
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

/// Converts the index into the neutral schema.
///
/// Names that fail platform validation are dropped rather than failing the
/// whole schema: a connector exposing one oddly-named internal table should not
/// stop the platform serving the rest. The dropped collection is then simply
/// unknown, which fails closed if a catalogue points at it.
fn build_neutral_schema(collections: &BTreeMap<String, BTreeMap<String, String>>) -> ConnectorSchema {
    ConnectorSchema::new(collections.iter().filter_map(|(collection_name, fields)| {
        let name = CollectionName::try_new(collection_name).ok()?;
        let field_names = fields.keys().filter_map(|field| FieldName::try_new(field).ok());

        Some((name, CollectionSchema::new(field_names)))
    }))
}

/// The union of neutral operators expressible on at least one scalar type.
fn supported_operators(
    operators: &BTreeMap<String, BTreeMap<SemanticOperator, String>>,
) -> BTreeSet<ComparisonOperator> {
    const CANDIDATES: [ComparisonOperator; 7] = [
        ComparisonOperator::Equal,
        ComparisonOperator::NotEqual,
        ComparisonOperator::LessThan,
        ComparisonOperator::LessThanOrEqual,
        ComparisonOperator::GreaterThan,
        ComparisonOperator::GreaterThanOrEqual,
        ComparisonOperator::Contains,
    ];

    CANDIDATES
        .into_iter()
        .filter(|candidate| {
            let semantic = SemanticOperator::for_neutral(*candidate);
            operators
                .values()
                .any(|by_semantic| by_semantic.contains_key(&semantic))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> NdcSchemaResponse {
        serde_json::from_str(
            r#"{
                "scalar_types": {
                    "int4": {"comparison_operators": {"_eq": {"type": "equal"}, "_lt": {"type": "less_than"}}},
                    "text": {
                        "comparison_operators": {
                            "_eq": {"type": "equal"},
                            "_ilike": {"type": "contains_insensitive"},
                            "_regex": {"type": "matches_regex"}
                        }
                    }
                },
                "object_types": {
                    "customers": {
                        "fields": {
                            "id": {"type": {"type": "named", "name": "int4"}},
                            "name": {"type": {"type": "nullable", "underlying_type": {"type": "named", "name": "text"}}}
                        }
                    }
                },
                "collections": [{"name": "customers", "type": "customers"}],
                "procedures": [{"name": "insert_customers"}]
            }"#,
        )
        .unwrap()
    }

    fn index() -> SchemaIndex {
        SchemaIndex::build(&schema())
    }

    fn customers() -> CollectionName {
        CollectionName::try_new("customers").unwrap()
    }

    fn field(name: &str) -> FieldName {
        FieldName::try_new(name).unwrap()
    }

    #[test]
    fn finds_the_connectors_own_name_for_equality() {
        assert_eq!(
            index().operator_name(&customers(), &field("id"), SemanticOperator::Equal),
            Some("_eq")
        );
    }

    #[test]
    fn resolves_operators_through_a_nullable_column_type() {
        assert_eq!(
            index().operator_name(&customers(), &field("name"), SemanticOperator::Equal),
            Some("_eq")
        );
    }

    #[test]
    fn an_operator_the_scalar_does_not_declare_is_absent_not_guessed() {
        // `text` declares no ordering operators, so there is no answer to give.
        assert_eq!(
            index().operator_name(&customers(), &field("name"), SemanticOperator::LessThan),
            None
        );
    }

    #[test]
    fn case_insensitive_containment_satisfies_containment() {
        assert_eq!(
            index().operator_name(&customers(), &field("name"), SemanticOperator::Contains),
            Some("_ilike")
        );
    }

    #[test]
    fn a_connector_specific_operator_is_not_mapped_to_anything() {
        // `_regex` has no standard semantic, so it must never be selected.
        let index = index();
        for semantic in [
            SemanticOperator::Equal,
            SemanticOperator::Contains,
            SemanticOperator::In,
        ] {
            assert_ne!(
                index.operator_name(&customers(), &field("name"), semantic),
                Some("_regex")
            );
        }
    }

    #[test]
    fn an_unknown_field_has_no_operator() {
        assert_eq!(
            index().operator_name(&customers(), &field("nonexistent"), SemanticOperator::Equal),
            None
        );
    }

    #[test]
    fn not_equal_is_reported_as_supported_wherever_equality_is() {
        // NDC has no not-equal semantic; it is a negated equality.
        assert!(index()
            .supported_operators()
            .contains(&ComparisonOperator::NotEqual));
    }

    #[test]
    fn the_neutral_schema_lists_the_collections_and_their_fields() {
        let index = index();
        let collection = index.neutral().collection(&customers()).unwrap();

        assert!(collection.has_field(&field("id")));
        assert!(collection.has_field(&field("name")));
    }

    #[test]
    fn procedures_are_recorded_for_mutation_validation() {
        assert!(index().has_procedure("insert_customers"));
        assert!(!index().has_procedure("drop_everything"));
    }
}
