//! The gate: an operation is checked against declared capabilities, and
//! refused if anything it needs is missing.
//!
//! Reads and writes deliberately funnel their predicate through one function.
//! A filter the read path refuses must never be one the write path executes,
//! and two copies of the same list of checks is precisely how that drifts.

use crate::{ConnectorCapabilities, ConnectorError, Filter, MutationSpec, QuerySpec, UnsupportedFeature};

impl ConnectorCapabilities {
    /// Checks a query against these capabilities before it is executed.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::Unsupported`] naming the first feature the
    /// backend lacks.
    pub fn ensure_supports_query(&self, spec: &QuerySpec) -> Result<(), ConnectorError> {
        if let Some(filter) = &spec.filter {
            self.ensure_supports_filter(filter, UnsupportedFeature::Filtering)?;
        }

        if !spec.sort.is_empty() && !self.ordering {
            return Err(UnsupportedFeature::Ordering.refused());
        }

        if (spec.limit.is_some() || spec.offset.is_some()) && !self.paging {
            return Err(UnsupportedFeature::Paging.refused());
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
            return Err(UnsupportedFeature::Mutations.refused());
        }

        let filter = match spec {
            MutationSpec::Insert { .. } => None,
            MutationSpec::Update { filter, .. } | MutationSpec::Delete { filter, .. } => filter.as_ref(),
        };

        // A predicate on a write is load-bearing in a way a read's is not: it is
        // what stops the write reaching another tenant's rows. If the backend
        // cannot express it, executing anyway would be destructive.
        if let Some(filter) = filter {
            self.ensure_supports_filter(filter, UnsupportedFeature::FilteringOnMutations)?;
        }

        Ok(())
    }

    /// Checks every capability a predicate depends on.
    ///
    /// `filtering_feature` only names the caller in the error message; the
    /// checks themselves are identical for reads and writes, because a
    /// predicate the backend cannot express is unsafe either way.
    ///
    /// Every refusal here is raised with
    /// [`refused`](UnsupportedFeature::refused) and no detail: the gate decides
    /// on a flag the connector declared, so there is no identifier to record
    /// that the operation's own log span does not already carry.
    fn ensure_supports_filter(
        &self,
        filter: &Filter,
        filtering_feature: UnsupportedFeature,
    ) -> Result<(), ConnectorError> {
        if !self.filtering {
            return Err(filtering_feature.refused());
        }

        for operator in filter.referenced_operators() {
            if !self.comparisons.contains(&operator) {
                return Err(UnsupportedFeature::Comparison(operator).refused());
            }
        }

        // Not folded into the loop above: a null test is not a comparison, so
        // it has no `ComparisonOperator` to look up. See
        // [`Filter::requires_null_check`].
        if filter.requires_null_check() && !self.null_checks {
            return Err(UnsupportedFeature::NullComparison.refused());
        }

        Ok(())
    }
}
