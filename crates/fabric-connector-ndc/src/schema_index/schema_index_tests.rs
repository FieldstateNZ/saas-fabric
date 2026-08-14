//! Operator discovery — the mechanism that keeps the client portable.

use fabric_connector::{CollectionName, ComparisonOperator, FieldName};

use crate::schema_index::{SchemaIndex, SemanticOperator};
use crate::wire::NdcSchemaResponse;

/// A schema arranged to exercise every lookup this index has to get right.
///
/// - `text` declares **both** `_like` and `_ilike`, so the exact-fit tie-break
///   has something to decide.
/// - `citext` declares only `_ilike`, so the widening fallback is still
///   reachable and can be told apart from the tie-break.
/// - `tags` is an `array<text>`, which has no scalar type of its own.
fn index() -> SchemaIndex {
    let schema: NdcSchemaResponse = serde_json::from_str(
        r#"{
            "scalar_types": {
                "int4": {"comparison_operators": {"_eq": {"type": "equal"}, "_lt": {"type": "less_than"}}},
                "text": {
                    "comparison_operators": {
                        "_eq": {"type": "equal"},
                        "_ilike": {"type": "contains_insensitive"},
                        "_like": {"type": "contains"},
                        "_regex": {"type": "matches_regex"}
                    }
                },
                "citext": {"comparison_operators": {"_ilike": {"type": "contains_insensitive"}}}
            },
            "object_types": {
                "customers": {
                    "fields": {
                        "id": {"type": {"type": "named", "name": "int4"}},
                        "name": {"type": {"type": "nullable", "underlying_type": {"type": "named", "name": "text"}}},
                        "label": {"type": {"type": "named", "name": "citext"}},
                        "tags": {"type": {"type": "array", "element_type": {"type": "named", "name": "text"}}}
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
fn case_insensitive_containment_satisfies_containment_where_nothing_exact_exists() {
    // `citext` declares only `_ilike`, so the widening is the only way to
    // offer containment at all. Safe because tenant scoping is always
    // equality on a discriminator, never containment.
    assert_eq!(
        index().operator_name(&customers(), &field("label"), SemanticOperator::Contains),
        Some("_ilike")
    );
}

#[test]
fn the_exact_containment_operator_beats_the_case_insensitive_one() {
    // `text` declares both. `_ilike` used to win because `BTreeMap` sorts it
    // first and both fold to `SemanticOperator::Contains` -- so a caller
    // asking for containment silently got the case-insensitive predicate,
    // decided by alphabetical order. `scalar-types.md` makes them different
    // operators, so the exact one has to win.
    assert_eq!(
        index().operator_name(&customers(), &field("name"), SemanticOperator::Contains),
        Some("_like")
    );
}

// -- Array columns: selectable, never comparable ---------------------------

#[test]
fn an_array_column_borrows_no_operator_from_its_element_type() {
    // `tags` is `array<text>` and `text` declares `_eq`. Unwrapping the array
    // put `text`'s `_eq` on the wire against the whole array -- which on a
    // document store conventionally means *contains*, strictly wider than
    // what was asked.
    let index = index();

    for semantic in [
        SemanticOperator::Equal,
        SemanticOperator::In,
        SemanticOperator::Contains,
    ] {
        assert_eq!(
            index.operator_name(&customers(), &field("tags"), semantic),
            None,
            "{semantic:?} resolved an operator for an array column"
        );
    }
}

#[test]
fn an_array_column_is_still_a_column() {
    // Only *filtering* an array is unsafe; reading one is fine. Dropping the
    // field from the index entirely would have removed it from the platform's
    // schema too, which is a wider loss than the fix needs.
    let index = index();

    assert!(index.has_field(&customers(), &field("tags")));
    assert!(index
        .neutral()
        .collection(&customers())
        .unwrap()
        .has_field(&field("tags")));
}

#[test]
fn a_column_the_connector_never_declared_is_absent() {
    assert!(!index().has_field(&customers(), &field("nonexistent")));
}

#[test]
fn a_column_on_an_unknown_collection_is_absent() {
    let unknown = CollectionName::try_new("orders").unwrap();

    assert!(!index().has_field(&unknown, &field("id")));
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
