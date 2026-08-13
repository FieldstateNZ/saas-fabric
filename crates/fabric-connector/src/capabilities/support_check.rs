//! The gate: an operation is checked against declared capabilities, and
//! refused if anything it needs is missing.
//!
//! Reads and writes deliberately funnel their predicate through one function.
//! A filter the read path refuses must never be one the write path executes,
//! and two copies of the same list of checks is precisely how that drifts.

use crate::{ConnectorCapabilities, ConnectorError, Filter, MutationSpec, QuerySpec};

impl ConnectorCapabilities {
    /// Checks a query against these capabilities before it is executed.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Unsupported`] naming the first feature the
    /// backend lacks.
    pub fn ensure_supports_query(&self, spec: &QuerySpec) -> Result<(), ConnectorError> {
        if let Some(filter) = &spec.filter {
            self.ensure_supports_filter(filter, "filtering")?;
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
            self.ensure_supports_filter(filter, "filtering on mutations")?;
        }

        Ok(())
    }

    /// Checks every capability a predicate depends on.
    ///
    /// `filtering_feature` only names the caller in the error message; the
    /// checks themselves are identical for reads and writes, because a
    /// predicate the backend cannot express is unsafe either way.
    fn ensure_supports_filter(&self, filter: &Filter, filtering_feature: &str) -> Result<(), ConnectorError> {
        if !self.filtering {
            return Err(unsupported(filtering_feature));
        }

        for operator in filter.referenced_operators() {
            if !self.comparisons.contains(&operator) {
                return Err(unsupported(&format!("the {} comparison", operator.as_str())));
            }
        }

        // Not folded into the loop above: a null test is not a comparison, so
        // it has no `ComparisonOperator` to look up. See
        // [`Filter::requires_null_check`].
        if filter.requires_null_check() && !self.null_checks {
            return Err(unsupported("null comparison"));
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
