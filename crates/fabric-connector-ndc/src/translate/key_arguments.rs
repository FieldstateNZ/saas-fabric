//! Reading a keyed procedure's key values off the predicate the platform
//! already built.

use std::collections::BTreeMap;

use fabric_connector::{ComparisonOperator, ConnectorError, Filter, MutationSpec};
use serde_json::Value;

use crate::config::ProcedureBinding;

/// Adds one argument per [`ProcedureBinding::key_arguments`] entry, sourced
/// from `filter`'s own equality clauses.
///
/// Called after the predicate has already been placed under
/// `filter_argument` (see `procedure_arguments::add_predicate`), on the exact
/// same, post-`for_target` filter — so a key value is read from the predicate
/// the platform built, **never** from the caller's payload and never guessed.
/// When a key field is also the tenant discriminator, its value reaches the
/// connector twice this way: once as a key argument, once inside the
/// predicate. That is defence in depth, not redundancy to trim.
///
/// # Errors
///
/// [`ConnectorError::InvalidOperation`] naming the verb, the collection, the
/// field, and the argument, when the filter has no equality for a mapped key
/// field, or more than one with differing values. A keyed procedure cannot be
/// called without its key, and it cannot be called against a contradictory
/// one either.
pub(super) fn add_key_arguments(
    arguments: &mut BTreeMap<String, Value>,
    binding: &ProcedureBinding,
    filter: &Filter,
    spec: &MutationSpec,
) -> Result<(), ConnectorError> {
    for (field, argument) in &binding.key_arguments {
        let value = single_equality(filter, field).map_err(|reason| {
            ConnectorError::InvalidOperation(format!(
                "a {} on {} maps key field `{field}` to argument `{argument}`, but {reason}",
                spec.operation_name(),
                spec.collection(),
            ))
        })?;

        arguments.insert(argument.clone(), value.clone());
    }

    Ok(())
}

/// The one value a filter's direct equality clauses agree `field` must have.
///
/// # Errors
///
/// A `&'static str` reason — no equality found, or more than one differing
/// value found — for the caller to fold into a message that also names the
/// verb, collection, field and argument this function does not know about.
fn single_equality<'a>(filter: &'a Filter, field: &str) -> Result<&'a Value, &'static str> {
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
fn direct_equalities<'a>(filter: &'a Filter, field: &str) -> Vec<&'a Value> {
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

#[cfg(test)]
mod tests {
    use fabric_connector::FieldName;

    use super::*;

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
}
