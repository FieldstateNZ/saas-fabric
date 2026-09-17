//! Tests for `key_equality`.

use fabric_connector::{ComparisonOperator, FieldName, Filter};
use serde_json::Value;

use super::key_equality::*;

fn compare(field: &str, operator: ComparisonOperator, value: &str) -> Filter {
    Filter::Compare {
        field: FieldName::try_new(field).unwrap(),
        operator,
        value: Value::String(value.to_owned()),
    }
}

fn equal(field: &str, value: &str) -> Filter {
    compare(field, ComparisonOperator::Equal, value)
}

#[test]
fn a_bare_top_level_equality_is_found() {
    let filter = equal("id", "9");

    assert_eq!(
        direct_equalities(&filter, "id"),
        vec![&Value::String("9".to_owned())]
    );
}

#[test]
fn a_direct_clause_of_a_top_level_and_is_found() {
    let filter = Filter::And {
        clauses: vec![equal("id", "9"), equal("tenant_key", "tenant-acme-482")],
    };

    assert_eq!(
        direct_equalities(&filter, "tenant_key"),
        vec![&Value::String("tenant-acme-482".to_owned())]
    );
}

#[test]
fn an_equality_hidden_under_an_or_is_not_found() {
    // A value that only holds along one branch of an `Or` is not a key
    // value — treating it as one would let one of two rows be reached
    // when only one was named.
    let filter = Filter::And {
        clauses: vec![Filter::Or {
            clauses: vec![equal("id", "9"), equal("id", "10")],
        }],
    };

    assert!(direct_equalities(&filter, "id").is_empty());
}

#[test]
fn an_equality_inside_a_nested_and_is_not_found() {
    let filter = Filter::And {
        clauses: vec![Filter::And {
            clauses: vec![equal("id", "9")],
        }],
    };

    assert!(direct_equalities(&filter, "id").is_empty());
}

#[test]
fn an_in_with_exactly_one_value_is_not_an_equality() {
    let filter = Filter::And {
        clauses: vec![Filter::In {
            field: FieldName::try_new("id").unwrap(),
            values: vec![Value::String("9".to_owned())],
        }],
    };

    assert!(direct_equalities(&filter, "id").is_empty());
}

#[test]
fn two_identical_equalities_are_treated_as_one() {
    let filter = Filter::And {
        clauses: vec![equal("id", "9"), equal("id", "9")],
    };

    assert_eq!(
        single_equality(&filter, "id").unwrap(),
        &Value::String("9".to_owned())
    );
}

#[test]
fn no_equality_at_all_is_refused() {
    let filter = equal("other_field", "9");

    assert!(single_equality(&filter, "id").is_err());
}

#[test]
fn two_differing_equalities_are_refused_as_contradictory() {
    let filter = Filter::And {
        clauses: vec![equal("id", "9"), equal("id", "10")],
    };

    let error = single_equality(&filter, "id").unwrap_err();

    assert!(error.contains("contradictory"), "{error}");
}
