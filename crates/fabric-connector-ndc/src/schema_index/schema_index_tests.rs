//! Operator discovery — the mechanism that keeps the client portable.

use fabric_connector::{CollectionName, ComparisonOperator, FieldName};

use crate::schema_index::{SchemaIndex, SemanticOperator};
use crate::wire::NdcSchemaResponse;

fn index() -> SchemaIndex {
    let schema: NdcSchemaResponse = serde_json::from_str(
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
    .unwrap();

    SchemaIndex::build(&schema)
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
fn a_connector_specific_operator_is_never_selected() {
    // `_regex` has no standard semantic, so nothing may map to it.
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
