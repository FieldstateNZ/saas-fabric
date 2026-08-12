//! Tests for expression.

use super::expression::*;
use crate::wire::NdcSchemaResponse;
use crate::wire::{NdcComparisonTarget, NdcComparisonValue, NdcExpression, NdcUnaryOperator};
use crate::SchemaIndex;
use fabric_connector::{CollectionName, ComparisonOperator, ConnectorError, FieldName, Filter};
use serde_json::Value;

fn index_from(scalar_operators: &str) -> SchemaIndex {
    let document = format!(
        r#"{{
            "scalar_types": {{"text": {{"comparison_operators": {scalar_operators}}}}},
            "object_types": {{"customers": {{"fields": {{
                "status": {{"type": {{"type": "named", "name": "text"}}}},
                "name": {{"type": {{"type": "named", "name": "text"}}}}
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

fn equality_only_index() -> SchemaIndex {
    index_from(r#"{"_eq": {"type": "equal"}}"#)
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
fn membership_uses_the_in_operator_when_the_connector_has_one() {
    let filter = Filter::In {
        field: field("status"),
        values: vec![Value::String("a".into()), Value::String("b".into())],
    };

    let NdcExpression::BinaryComparisonOperator { operator, value, .. } =
        to_expression(&customers(), &filter, &full_index()).unwrap()
    else {
        panic!("expected a binary comparison");
    };
    assert_eq!(operator, "_in");
    assert_eq!(
        value,
        NdcComparisonValue::scalar(Value::Array(vec![
            Value::String("a".into()),
            Value::String("b".into())
        ]))
    );
}

#[test]
fn membership_falls_back_to_a_disjunction_of_equalities() {
    let filter = Filter::In {
        field: field("status"),
        values: vec![Value::String("a".into()), Value::String("b".into())],
    };

    let NdcExpression::Or { expressions } =
        to_expression(&customers(), &filter, &equality_only_index()).unwrap()
    else {
        panic!("expected a disjunction");
    };
    assert_eq!(expressions.len(), 2);
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
