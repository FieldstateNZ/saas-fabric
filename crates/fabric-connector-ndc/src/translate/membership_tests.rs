//! Tests for membership.

use super::expression::to_expression;
use crate::wire::{NdcComparisonValue, NdcExpression, NdcSchemaResponse};
use crate::SchemaIndex;
use fabric_connector::{CollectionName, ConnectorError, FieldName, Filter};
use serde_json::Value;

fn index_from(scalar_operators: &str) -> SchemaIndex {
    let document = format!(
        r#"{{
            "scalar_types": {{"text": {{"comparison_operators": {scalar_operators}}}}},
            "object_types": {{"customers": {{"fields": {{
                "status": {{"type": {{"type": "named", "name": "text"}}}}
            }}}}}},
            "collections": [{{"name": "customers", "type": "customers"}}],
            "procedures": []
        }}"#
    );

    let schema: NdcSchemaResponse = serde_json::from_str(&document).unwrap();
    SchemaIndex::build(&schema)
}

fn full_index() -> SchemaIndex {
    index_from(r#"{"_eq": {"type": "equal"}, "_in": {"type": "in"}}"#)
}

fn equality_only_index() -> SchemaIndex {
    index_from(r#"{"_eq": {"type": "equal"}}"#)
}

fn customers() -> CollectionName {
    CollectionName::try_new("customers").unwrap()
}

fn membership(field: &str, values: Vec<Value>) -> Filter {
    Filter::In {
        field: FieldName::try_new(field).unwrap(),
        values,
    }
}

fn two_values() -> Vec<Value> {
    vec![Value::String("a".into()), Value::String("b".into())]
}

#[test]
fn membership_uses_the_in_operator_when_the_connector_has_one() {
    let NdcExpression::BinaryComparisonOperator { operator, value, .. } =
        to_expression(&customers(), &membership("status", two_values()), &full_index()).unwrap()
    else {
        panic!("expected a binary comparison");
    };

    assert_eq!(operator, "_in");
    assert_eq!(value, NdcComparisonValue::scalar(Value::Array(two_values())));
}

#[test]
fn membership_falls_back_to_a_disjunction_of_equalities() {
    let NdcExpression::Or { expressions } = to_expression(
        &customers(),
        &membership("status", two_values()),
        &equality_only_index(),
    )
    .unwrap() else {
        panic!("expected a disjunction");
    };

    assert_eq!(expressions.len(), 2);
}

#[test]
fn membership_of_an_empty_set_is_refused_rather_than_satisfied() {
    // It used to emit `or[]`. Narrowing, so not a leak — but it was the one
    // path here that reached the wire without a single schema lookup.
    assert!(matches!(
        to_expression(&customers(), &membership("status", Vec::new()), &full_index()).unwrap_err(),
        ConnectorError::InvalidOperation(_)
    ));
}

#[test]
fn an_empty_membership_test_is_refused_on_the_fallback_path_too() {
    // Both paths, or the refusal depends on which operators the connector
    // happens to declare.
    assert!(to_expression(
        &customers(),
        &membership("status", Vec::new()),
        &equality_only_index()
    )
    .is_err());
}

#[test]
fn an_empty_membership_test_no_longer_filters_on_a_column_that_need_not_exist() {
    // The concrete symptom: no schema lookup ran, so the column was never
    // checked against the connector's own object type.
    assert!(to_expression(
        &customers(),
        &membership("column_that_does_not_exist", Vec::new()),
        &equality_only_index()
    )
    .is_err());
}
