//! What a predicate asks of whatever will run it.
//!
//! Separated from the enum's own file because these walks share a hazard the
//! shape does not: each one must account for **every** variant, and forgetting
//! a variant produces no compile error — only a check that quietly passes
//! things it should have refused. Keeping them adjacent means a new variant has
//! one obvious place to be considered.

use crate::{ComparisonOperator, FieldName, Filter};

impl Filter {
    /// Every field this predicate mentions.
    ///
    /// Used to check a filter against the connector's schema before executing:
    /// a filter naming a field that does not exist should be a clean rejection,
    /// not a backend error with an unpredictable message.
    #[must_use]
    pub fn referenced_fields(&self) -> Vec<&FieldName> {
        let mut fields = Vec::new();
        self.collect_fields(&mut fields);
        fields
    }

    /// Walks the tree accumulating field references.
    fn collect_fields<'a>(&'a self, into: &mut Vec<&'a FieldName>) {
        match self {
            Self::And { clauses } | Self::Or { clauses } => {
                for clause in clauses {
                    clause.collect_fields(into);
                }
            }
            Self::Not { clause } => clause.collect_fields(into),
            Self::Compare { field, .. } | Self::IsNull { field } | Self::In { field, .. } => {
                into.push(field);
            }
        }
    }

    /// Every comparison the backend must be able to express to run this
    /// predicate *as written*.
    ///
    /// Deliberately not a literal inventory of the [`Filter::Compare`] nodes:
    /// it is the set that must appear in
    /// [`ConnectorCapabilities::comparisons`](crate::ConnectorCapabilities)
    /// before execution is allowed. A variant that is not itself a `Compare`
    /// still contributes whatever a faithful rewrite of it would need.
    ///
    /// [`Filter::IsNull`] is the one thing no comparison can stand in for, so
    /// it is reported separately by [`Self::requires_null_check`].
    #[must_use]
    pub fn referenced_operators(&self) -> Vec<ComparisonOperator> {
        let mut operators = Vec::new();
        self.collect_operators(&mut operators);
        operators.sort_unstable();
        operators.dedup();
        operators
    }

    /// Walks the tree accumulating operators.
    fn collect_operators(&self, into: &mut Vec<ComparisonOperator>) {
        match self {
            Self::And { clauses } | Self::Or { clauses } => {
                for clause in clauses {
                    clause.collect_operators(into);
                }
            }
            Self::Not { clause } => clause.collect_operators(into),
            Self::Compare { operator, .. } => into.push(*operator),

            // `x IN (a, b)` and `x = a OR x = b` select the same rows, and
            // disjunction needs no capability of its own — it falls under the
            // coarse `filtering` flag. So equality is the whole of what
            // membership additionally demands, and charging for it is not an
            // approximation. It is also what actually runs: when a connector
            // declares no native `in`, `fabric-connector-ndc` translates
            // membership to exactly this disjunction.
            //
            // An empty `values` list is charged too. The requirement belongs to
            // the shape, not to caller-supplied data, and a check whose
            // strictness varied with list length would be one more thing to
            // reason about at the boundary that must fail closed (§28).
            Self::In { .. } => into.push(ComparisonOperator::Equal),

            // Nothing: null testing is not a comparison against a literal and
            // has no sound rewrite into one. Under three-valued logic
            // `x = NULL` is unknown for every row, so a backend proving it can
            // compare for equality has proved nothing about finding nulls.
            // `requires_null_check` reports it against its own capability.
            Self::IsNull { .. } => {}
        }
    }

    /// Whether any clause tests a field for null.
    ///
    /// Checked against
    /// [`ConnectorCapabilities::null_checks`](crate::ConnectorCapabilities)
    /// before execution. Separate from [`Self::referenced_operators`] because a
    /// null test is unary: there is no literal to compare against, so it cannot
    /// be modelled as a [`ComparisonOperator`] without making
    /// `Compare { operator, value }` carry a value that means nothing.
    #[must_use]
    pub fn requires_null_check(&self) -> bool {
        match self {
            Self::And { clauses } | Self::Or { clauses } => clauses.iter().any(Self::requires_null_check),
            Self::Not { clause } => clause.requires_null_check(),
            Self::IsNull { .. } => true,
            Self::Compare { .. } | Self::In { .. } => false,
        }
    }
}
