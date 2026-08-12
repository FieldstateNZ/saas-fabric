//! Combining, flattening, and walking the predicate tree.

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

#[test]
fn combining_two_leaves_produces_a_two_clause_conjunction() {
    let combined = compare("a", ComparisonOperator::Equal).and(compare("b", ComparisonOperator::Equal));

    let Filter::And { clauses } = combined else {
        panic!("expected a conjunction");
    };
    assert_eq!(clauses.len(), 2);
}

#[test]
fn combining_flattens_rather_than_nesting() {
    // A tenant predicate joined onto an existing conjunction must not produce
    // And[And[..], ..] — that nests one level deeper on every call.
    let existing = compare("a", ComparisonOperator::Equal).and(compare("b", ComparisonOperator::Equal));

    let Filter::And { clauses } = existing.and(compare("tenant", ComparisonOperator::Equal)) else {
        panic!("expected a conjunction");
    };
    assert_eq!(clauses.len(), 3);
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
