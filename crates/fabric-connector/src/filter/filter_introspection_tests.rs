//! What a predicate reports it needs. Under-reporting here is what lets a
//! capability check pass a filter the backend cannot actually express.

use serde_json::Value;

use crate::testing::field;
use crate::{ComparisonOperator, Filter};

fn compare(name: &str, operator: ComparisonOperator) -> Filter {
    Filter::Compare {
        field: field(name),
        operator,
        value: Value::Bool(true),
    }
}

fn membership(name: &str) -> Filter {
    Filter::In {
        field: field(name),
        values: vec![Value::String("au".to_owned()), Value::String("nz".to_owned())],
    }
}

#[test]
fn collects_fields_from_nested_clauses() {
    let filter = Filter::Not {
        clause: Box::new(Filter::Or {
            clauses: vec![
                compare("a", ComparisonOperator::Equal),
                Filter::IsNull { field: field("b") },
            ],
        }),
    };

    let fields: Vec<&str> = filter.referenced_fields().iter().map(|f| f.as_str()).collect();

    assert_eq!(fields, ["a", "b"]);
}

#[test]
fn collects_distinct_operators() {
    let filter = compare("a", ComparisonOperator::Contains)
        .and(compare("b", ComparisonOperator::Contains))
        .and(compare("c", ComparisonOperator::GreaterThan));

    assert_eq!(
        filter.referenced_operators(),
        [ComparisonOperator::GreaterThan, ComparisonOperator::Contains]
    );
}

#[test]
fn membership_requires_the_equality_comparison() {
    // `x IN (au, nz)` is `x = au OR x = nz`. Reporting nothing here is what
    // let a connector with no equality operator be handed an `In` filter.
    assert_eq!(
        membership("region").referenced_operators(),
        [ComparisonOperator::Equal]
    );
}

#[test]
fn membership_of_an_empty_set_still_requires_equality() {
    // The requirement is a property of the shape, not of the caller's data.
    let empty = Filter::In {
        field: field("region"),
        values: Vec::new(),
    };

    assert_eq!(empty.referenced_operators(), [ComparisonOperator::Equal]);
}

#[test]
fn membership_buried_in_a_nested_tree_is_still_reported() {
    let filter = Filter::Not {
        clause: Box::new(Filter::Or {
            clauses: vec![membership("region")],
        }),
    };

    assert_eq!(filter.referenced_operators(), [ComparisonOperator::Equal]);
}

#[test]
fn a_null_test_is_reported_separately_from_the_comparisons() {
    // It must not masquerade as an equality: a backend that can compare for
    // equality has not thereby shown it can find nulls.
    let filter = Filter::IsNull {
        field: field("archived_at"),
    };

    assert!(filter.requires_null_check());
    assert!(filter.referenced_operators().is_empty());
}

#[test]
fn a_null_test_buried_in_a_nested_tree_is_still_reported() {
    let filter = Filter::And {
        clauses: vec![Filter::Not {
            clause: Box::new(Filter::Or {
                clauses: vec![
                    compare("a", ComparisonOperator::Equal),
                    Filter::IsNull { field: field("b") },
                ],
            }),
        }],
    };

    assert!(filter.requires_null_check());
}

#[test]
fn a_predicate_with_no_null_test_does_not_demand_the_capability() {
    // The check must not be so blunt that every filter needs `null_checks`.
    let filter = compare("a", ComparisonOperator::Equal).and(membership("region"));

    assert!(!filter.requires_null_check());
}
