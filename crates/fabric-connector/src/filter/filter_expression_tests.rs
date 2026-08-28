//! Combining and flattening the predicate tree.

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
