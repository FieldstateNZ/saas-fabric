//! Reading the one value a predicate's direct equality clauses agree a field
//! must have.

use fabric_connector::{ComparisonOperator, Filter};
use serde_json::Value;

/// The one value a filter's direct equality clauses agree `field` must have.
///
/// # Errors
///
/// A `&'static str` reason — no equality found, or more than one differing
/// value found — for the caller to fold into a message that also names the
/// verb, collection, field and argument this function does not know about.
///
/// `pub(super)` rather than private: `translate::key_equality_tests` is a
/// sibling module, not a child of this one, and exercises this function
/// directly rather than only through `key_arguments::add_key_arguments` — see
/// that module's own tests for why the distinction between "no equality" and
/// "contradictory equalities" is worth pinning on its own.
pub(super) fn single_equality<'a>(filter: &'a Filter, field: &str) -> Result<&'a Value, &'static str> {
    let mut distinct: Vec<&Value> = Vec::new();

    for value in direct_equalities(filter, field) {
        if !distinct.contains(&value) {
            distinct.push(value);
        }
    }

    match distinct.as_slice() {
        [] => Err(
            "the predicate has no equality for that field; a keyed procedure cannot be called without its key",
        ),
        [value] => Ok(value),
        _ => Err(
            "the predicate has more than one differing equality for that field; a keyed procedure cannot be \
             called against a contradictory predicate",
        ),
    }
}

/// The equality comparisons made directly against `field`: a bare
/// `Filter::Compare` at the top of the tree, or the direct clauses of a
/// top-level `Filter::And`.
///
/// Deliberately shallow. Descending into `Or`, `Not`, or a nested `And` would
/// treat a value that only holds along one branch of the predicate as though
/// it always held — which is exactly the shape of predicate a keyed procedure
/// cannot safely be called against. `Filter::In` is excluded even with
/// exactly one value: a set the platform never collapsed to an equality is
/// not one, and keeping this strict costs nothing real callers need.
///
/// `pub(super)`, for the same reason as [`single_equality`] above.
pub(super) fn direct_equalities<'a>(filter: &'a Filter, field: &str) -> Vec<&'a Value> {
    match filter {
        Filter::Compare {
            field: candidate,
            operator: ComparisonOperator::Equal,
            value,
        } if candidate.as_str() == field => vec![value],
        Filter::And { clauses } => clauses
            .iter()
            .filter_map(|clause| match clause {
                Filter::Compare {
                    field: candidate,
                    operator: ComparisonOperator::Equal,
                    value,
                } if candidate.as_str() == field => Some(value),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}
