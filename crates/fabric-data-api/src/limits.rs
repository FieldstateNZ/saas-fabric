//! Enforcing the Data API's complexity and size bounds (§28).
//!
//! Every bound configured on [`DataApiConfig`] is checked here, and every
//! check runs before [`DataApiService`](crate::DataApiService) ever reaches a
//! connector. Five of the six live in this file: how many equality filters,
//! sort fields, and projected fields one list request may carry; how deep the
//! resulting filter tree may nest; and how many rows one mutation may write.
//! The sixth — request body size — is enforced earlier still, while the body
//! is being read (`extraction::BoundedJson`), because a request already over
//! the byte limit should never reach a parser at all.

use fabric_connector::Filter;

use crate::{DataApiConfig, DataApiError, ListQuery};

/// Rejects a parsed list query that exceeds any configured complexity bound.
///
/// # Errors
///
/// [`DataApiError::BadRequest`] naming whichever bound was exceeded.
pub(crate) fn enforce_query(query: &ListQuery, config: &DataApiConfig) -> Result<(), DataApiError> {
    within("filters", query.filters.len(), config.max_filters)?;
    within("sort fields", query.sort.len(), config.max_sort_fields)?;
    within("select fields", query.select.len(), config.max_select_fields)?;

    if let Some(filter) = query.to_filter() {
        within("filter depth", depth(&filter), config.max_filter_depth)?;
    }

    Ok(())
}

/// Rejects a mutation batch over its configured maximum.
///
/// # Errors
///
/// [`DataApiError::BadRequest`] if `rows` exceeds `max_mutation_batch_size`.
pub(crate) fn enforce_batch_size(rows: usize, config: &DataApiConfig) -> Result<(), DataApiError> {
    within("records in one request", rows, config.max_mutation_batch_size)
}

/// Rejects `actual` if it exceeds `max`, naming the boundary in the message.
///
/// The public message is safe to return verbatim: it names a count and a
/// caller-facing label, never anything physical.
fn within(what: &str, actual: usize, max: u32) -> Result<(), DataApiError> {
    if actual > usize::try_from(max).unwrap_or(usize::MAX) {
        return Err(DataApiError::BadRequest(format!(
            "too many {what}: at most {max} allowed"
        )));
    }

    Ok(())
}

/// The nesting depth of a filter tree.
///
/// A comparison, null check, or set membership test is depth one; a
/// conjunction, disjunction, or negation is one more than its deepest child.
/// The query language this crate parses never builds anything past an `And`
/// of comparisons — depth two — but the function is written for the general
/// tree so a future addition to the language is bounded automatically.
fn depth(filter: &Filter) -> usize {
    match filter {
        Filter::And { clauses } | Filter::Or { clauses } => 1 + clauses.iter().map(depth).max().unwrap_or(0),
        Filter::Not { clause } => 1 + depth(clause),
        Filter::Compare { .. } | Filter::IsNull { .. } | Filter::In { .. } => 1,
    }
}

#[cfg(test)]
mod tests {
    use fabric_connector::{ComparisonOperator, FieldName};
    use serde_json::Value;

    use super::*;

    fn field(name: &str) -> FieldName {
        FieldName::try_new(name).unwrap()
    }

    fn compare(name: &str) -> Filter {
        Filter::Compare {
            field: field(name),
            operator: ComparisonOperator::Equal,
            value: Value::String("x".to_owned()),
        }
    }

    #[test]
    fn a_single_comparison_is_depth_one() {
        assert_eq!(depth(&compare("id")), 1);
    }

    #[test]
    fn a_conjunction_of_comparisons_is_depth_two() {
        let filter = Filter::And {
            clauses: vec![compare("id"), compare("name")],
        };

        assert_eq!(depth(&filter), 2);
    }

    #[test]
    fn a_negation_adds_one_level_to_its_child() {
        let filter = Filter::Not {
            clause: Box::new(compare("id")),
        };

        assert_eq!(depth(&filter), 2);
    }

    #[test]
    fn a_count_at_the_limit_is_accepted() {
        assert!(within("things", 5, 5).is_ok());
    }

    #[test]
    fn a_count_one_over_the_limit_is_rejected() {
        assert!(within("things", 6, 5).is_err());
    }
}
