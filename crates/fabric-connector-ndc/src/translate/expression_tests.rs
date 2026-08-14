//! Tests for expression.

use super::expression::*;
use crate::wire::NdcSchemaResponse;
use crate::wire::{NdcComparisonTarget, NdcExpression, NdcUnaryOperator};
use crate::SchemaIndex;
use fabric_connector::{CollectionName, ComparisonOperator, ConnectorError, FieldName, Filter};
use serde_json::Value;

fn index_from(scalar_operators: &str) -> SchemaIndex {
    let document = format!(
        r#"{{
            "scalar_types": {{"text": {{"comparison_operators": {scalar_operators}}}}},
            "object_types": {{"customers": {{"fields": {{
                "status": {{"type": {{"type": "named", "name": "text"}}}},
                "name": {{"type": {{"type": "named", "name": "text"}}}},
                "tags": {{"type": {{"type": "array", "element_type": {{"type": "named", "name": "text"}}}}}}
            }}}}}},
            "collections": [{{"name": "customers", "type": "customers"}}],
            "procedures": []
        }}"#
    );

    let schema: NdcSchemaResponse = serde_json::from_str(&document).unwrap();
    SchemaIndex::build(&schema)
}

fn full_index() -> SchemaIndex {
    index_from(
        r#"{"_eq": {"type": "equal"}, "_in": {"type": "in"}, "_ilike": {"type": "contains_insensitive"}}"#,
    )
}

fn customers() -> CollectionName {
    CollectionName::try_new("customers").unwrap()
}

fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}

fn equals(name: &str, value: &str) -> Filter {
    Filter::Compare {
        field: field(name),
        operator: ComparisonOperator::Equal,
        value: Value::String(value.to_owned()),
    }
}

#[test]
fn an_equality_uses_the_connectors_operator_name() {
    let expression = to_expression(&customers(), &equals("status", "active"), &full_index()).unwrap();

    let NdcExpression::BinaryComparisonOperator { operator, column, .. } = expression else {
        panic!("expected a binary comparison");
    };
    assert_eq!(operator, "_eq");
    assert_eq!(column, NdcComparisonTarget::column("status"));
}

#[test]
fn an_inequality_becomes_a_negated_equality() {
    // NDC has no not-equal semantic.
    let filter = Filter::Compare {
        field: field("status"),
        operator: ComparisonOperator::NotEqual,
        value: Value::String("archived".to_owned()),
    };

    let NdcExpression::Not { expression } = to_expression(&customers(), &filter, &full_index()).unwrap()
    else {
        panic!("expected a negation");
    };
    assert!(matches!(
        *expression,
        NdcExpression::BinaryComparisonOperator { .. }
    ));
}

#[test]
fn a_null_check_becomes_a_unary_comparison() {
    let filter = Filter::IsNull { field: field("name") };

    assert!(matches!(
        to_expression(&customers(), &filter, &full_index()).unwrap(),
        NdcExpression::UnaryComparisonOperator {
            operator: NdcUnaryOperator::IsNull,
            ..
        }
    ));
}

#[test]
fn a_null_check_on_a_column_the_connector_never_declared_is_refused() {
    // This was the last path in the translator that reached the wire having
    // consulted the schema not at all.
    let filter = Filter::IsNull {
        field: field("no_such_column"),
    };

    let error = to_expression(&customers(), &filter, &full_index()).unwrap_err();

    let ConnectorError::Unsupported { feature, .. } = &error else {
        panic!("expected Unsupported, got {error:?}");
    };
    assert_eq!(feature.as_str(), "null comparison");
}

#[test]
fn a_null_check_on_an_array_column_is_allowed() {
    // Existence is what a null test needs, not comparability. `is_null` is a
    // core NDC unary operator and applies whatever the column's type is --
    // including an array, which has no scalar type at all.
    let filter = Filter::IsNull { field: field("tags") };

    assert!(matches!(
        to_expression(&customers(), &filter, &full_index()).unwrap(),
        NdcExpression::UnaryComparisonOperator {
            operator: NdcUnaryOperator::IsNull,
            ..
        }
    ));
}

// -- Array columns are refused, not silently compared with element operators

#[test]
fn a_comparison_against_an_array_column_is_refused_rather_than_widened() {
    // `text` declares `_eq` and `tags` is `array<text>`. Emitting `text`'s
    // `_eq` here means *contains* on a document store -- strictly wider than
    // the equality the caller asked for, and it fails in the direction that
    // returns rows rather than an error.
    let error = to_expression(&customers(), &equals("tags", "vip"), &full_index()).unwrap_err();

    assert!(matches!(error, ConnectorError::Unsupported { .. }));
}

#[test]
fn membership_against_an_array_column_is_refused_through_both_of_its_paths() {
    // `_in` is declared, so the first path is tried; without it the fallback
    // is a disjunction of equalities, which must fail for the same reason.
    let values = vec![Value::String("vip".to_owned())];

    for index in [full_index(), index_from(r#"{"_eq": {"type": "equal"}}"#)] {
        let filter = Filter::In {
            field: field("tags"),
            values: values.clone(),
        };

        assert!(
            matches!(
                to_expression(&customers(), &filter, &index),
                Err(ConnectorError::Unsupported { .. })
            ),
            "an array membership test was not refused"
        );
    }
}

#[test]
fn an_operator_the_connector_cannot_express_is_refused() {
    let filter = Filter::Compare {
        field: field("status"),
        operator: ComparisonOperator::GreaterThan,
        value: Value::from(1),
    };

    let error = to_expression(&customers(), &filter, &full_index()).unwrap_err();
    assert!(matches!(error, ConnectorError::Unsupported { .. }));
}

#[test]
fn a_nested_clause_that_cannot_be_expressed_fails_the_whole_predicate() {
    // Partial translation would silently widen the result set.
    let filter = Filter::And {
        clauses: vec![
            equals("status", "active"),
            Filter::Compare {
                field: field("status"),
                operator: ComparisonOperator::LessThan,
                value: Value::from(1),
            },
        ],
    };

    assert!(to_expression(&customers(), &filter, &full_index()).is_err());
}

#[test]
fn a_conjunction_translates_every_clause() {
    let filter = equals("status", "active").and(equals("name", "Alice"));

    let NdcExpression::And { expressions } = to_expression(&customers(), &filter, &full_index()).unwrap()
    else {
        panic!("expected a conjunction");
    };
    assert_eq!(expressions.len(), 2);
}
