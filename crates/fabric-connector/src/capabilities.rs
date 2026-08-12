//! What a given backend can actually do.

use std::collections::BTreeSet;

use crate::{ComparisonOperator, ConnectorError, MutationSpec, QuerySpec};

/// The features a connector declares support for.
///
/// # Why this is checked rather than assumed
///
/// Backends differ. A document store may not express the same predicates as
/// PostgreSQL; a read-only analytics connector may support no mutations at all.
/// There are two ways to handle that, and only one is safe in a multi-tenant
/// system.
///
/// The unsafe way is to degrade quietly — drop a predicate the backend cannot
/// express and return what you get. In a single-tenant application that is
/// merely wrong. Here, the predicate that gets dropped might be the one
/// restricting rows to the caller's tenant, and the failure looks exactly like
/// success: rows come back, the status code is 200, and nothing is logged.
///
/// So the platform refuses instead. §28 requires failing closed, and an
/// unsupported operation is a case where the safe answer cannot be computed.
// A flag per capability is the honest representation here: these are
// independent yes/no facts a backend declares, not a state machine that could
// be an enum. Grouping them into sub-structs to satisfy the lint would add
// nesting at every call site and hide nothing.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorCapabilities {
    /// Whether predicates may be pushed down at all.
    pub filtering: bool,
    /// Whether ordering may be pushed down.
    pub ordering: bool,
    /// Whether `limit`/`offset` may be pushed down.
    pub paging: bool,
    /// Whether the backend accepts writes.
    pub mutations: bool,
    /// Whether several mutations in one request are atomic.
    pub transactional_mutations: bool,
    /// Whether the backend can report a total row count ignoring paging.
    pub total_count: bool,
    /// The comparison operators the backend can express.
    pub comparisons: BTreeSet<ComparisonOperator>,
}

impl ConnectorCapabilities {
    /// The minimum a connector must support to be useful: reads with
    /// predicates, ordering, paging, and equality.
    #[must_use]
    pub fn baseline() -> Self {
        Self {
            filtering: true,
            ordering: true,
            paging: true,
            mutations: false,
            transactional_mutations: false,
            total_count: false,
            comparisons: [ComparisonOperator::Equal, ComparisonOperator::NotEqual]
                .into_iter()
                .collect(),
        }
    }

    /// Checks a query against these capabilities before it is executed.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Unsupported`] naming the first feature the
    /// backend lacks.
    pub fn ensure_supports_query(&self, spec: &QuerySpec) -> Result<(), ConnectorError> {
        if let Some(filter) = &spec.filter {
            if !self.filtering {
                return Err(unsupported("filtering"));
            }

            for operator in filter.referenced_operators() {
                if !self.comparisons.contains(&operator) {
                    return Err(unsupported(&format!("the {} comparison", operator.as_str())));
                }
            }
        }

        if !spec.sort.is_empty() && !self.ordering {
            return Err(unsupported("ordering"));
        }

        if (spec.limit.is_some() || spec.offset.is_some()) && !self.paging {
            return Err(unsupported("paging"));
        }

        Ok(())
    }

    /// Checks a mutation against these capabilities before it is executed.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Unsupported`] if the backend does not accept
    /// writes, or cannot express a predicate the mutation depends on.
    pub fn ensure_supports_mutation(&self, spec: &MutationSpec) -> Result<(), ConnectorError> {
        if !self.mutations {
            return Err(unsupported("mutations"));
        }

        let filter = match spec {
            MutationSpec::Insert { .. } => None,
            MutationSpec::Update { filter, .. } | MutationSpec::Delete { filter, .. } => filter.as_ref(),
        };

        // A predicate on a write is load-bearing in a way a read's is not: it is
        // what stops the write reaching another tenant's rows. If the backend
        // cannot express it, executing anyway would be destructive.
        if let Some(filter) = filter {
            if !self.filtering {
                return Err(unsupported("filtering on mutations"));
            }

            for operator in filter.referenced_operators() {
                if !self.comparisons.contains(&operator) {
                    return Err(unsupported(&format!("the {} comparison", operator.as_str())));
                }
            }
        }

        Ok(())
    }
}

/// Builds the standard unsupported-feature error.
fn unsupported(feature: &str) -> ConnectorError {
    ConnectorError::Unsupported {
        feature: feature.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::{CollectionName, FieldName, Filter, Row, SortField};

    fn collection() -> CollectionName {
        CollectionName::try_new("customers").unwrap()
    }

    fn contains_filter() -> Filter {
        Filter::Compare {
            field: FieldName::try_new("name").unwrap(),
            operator: ComparisonOperator::Contains,
            value: Value::String("Ali".to_owned()),
        }
    }

    #[test]
    fn the_baseline_accepts_a_simple_equality_query() {
        let spec = QuerySpec::new(collection()).with_filter(Filter::Compare {
            field: FieldName::try_new("id").unwrap(),
            operator: ComparisonOperator::Equal,
            value: Value::from(1),
        });

        assert!(ConnectorCapabilities::baseline()
            .ensure_supports_query(&spec)
            .is_ok());
    }

    #[test]
    fn a_comparison_the_backend_cannot_express_is_refused_not_dropped() {
        let spec = QuerySpec::new(collection()).with_filter(contains_filter());

        let error = ConnectorCapabilities::baseline()
            .ensure_supports_query(&spec)
            .unwrap_err();

        assert!(matches!(error, ConnectorError::Unsupported { .. }));
    }

    #[test]
    fn ordering_is_refused_when_the_backend_cannot_sort() {
        let capabilities = ConnectorCapabilities {
            ordering: false,
            ..ConnectorCapabilities::baseline()
        };
        let spec = QuerySpec::new(collection())
            .with_sort(vec![SortField::ascending(FieldName::try_new("id").unwrap())]);

        assert!(capabilities.ensure_supports_query(&spec).is_err());
    }

    #[test]
    fn a_read_only_connector_refuses_every_mutation() {
        let spec = MutationSpec::Insert {
            collection: collection(),
            rows: vec![Row::new()],
        };

        assert!(ConnectorCapabilities::baseline()
            .ensure_supports_mutation(&spec)
            .is_err());
    }

    #[test]
    fn a_delete_whose_predicate_the_backend_cannot_express_is_refused() {
        // Executing this anyway would delete rows the predicate was meant to
        // protect — including, under discriminator isolation, other tenants'.
        let capabilities = ConnectorCapabilities {
            mutations: true,
            ..ConnectorCapabilities::baseline()
        };
        let spec = MutationSpec::Delete {
            collection: collection(),
            filter: Some(contains_filter()),
        };

        assert!(capabilities.ensure_supports_mutation(&spec).is_err());
    }
}
