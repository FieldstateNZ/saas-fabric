//! The closed vocabulary a refused caller may be told.

use std::fmt;

use crate::{ComparisonOperator, ConnectorError, RefusalDetail};

/// A capability name that is safe to return to an application.
///
/// # Why this is an enum
///
/// [`ConnectorError::Unsupported`] is the **one** connector error whose text
/// `fabric-data-api` forwards to a caller; every sibling is masked to "internal
/// error". While that text was a `String`, staying safe meant every connector
/// author remembering not to interpolate what they were translating — and that
/// is precisely what failed. A real refusal reached a 400 body reading
/// `comparing customer_records_v2.tenant_key with a Equal operator`, naming a
/// shared table *and* the column holding the tenant boundary up (§2, §29). The
/// predicate case is the sharp one: translation runs after
/// [`QuerySpec::for_target`](crate::QuerySpec::for_target) has conjoined the
/// discriminator, so a refusal there is raised on a predicate the caller never
/// wrote, over the isolation column itself.
///
/// A closed set makes that unrepresentable rather than discouraged: there is
/// nowhere in this type to put a collection, field, or procedure name.
///
/// # The rule for adding a variant
///
/// [`as_str`](Self::as_str) returns `&'static str`, and that return type is
/// load-bearing. A variant carrying runtime text could not be rendered from it,
/// so the compiler refuses the shape rather than trusting the author to keep
/// the payload harmless. Keep it that way: what a caller receives is always a
/// literal compiled into this crate.
///
/// Names say what capability the *caller* asked for, never where it was needed.
/// "writes to this collection" is safe because the caller already knows which
/// collection it named; the physical one belongs in [`RefusalDetail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedFeature {
    /// Predicates at all.
    Filtering,
    /// Predicates on a write. Distinguished because a write refused for want
    /// of a predicate is the more serious of the two.
    FilteringOnMutations,
    /// Ordering.
    Ordering,
    /// Limit or offset.
    Paging,
    /// One binary comparison operator.
    Comparison(ComparisonOperator),
    /// Set membership (`in`).
    Membership,
    /// Testing a field for null. Not a comparison: it is unary, and under
    /// three-valued logic equality support proves nothing about it.
    NullComparison,
    /// Writes at all.
    Mutations,
    /// Writes to the collection the caller named.
    WritesToCollection,
    /// Inserts on the collection the caller named.
    InsertsOnCollection,
    /// Updates on the collection the caller named.
    UpdatesOnCollection,
    /// Deletes on the collection the caller named.
    DeletesOnCollection,
}

impl UnsupportedFeature {
    /// This crate's own words for the capability.
    ///
    /// `&'static str` on purpose — see the type docs. Nothing a connector, a
    /// schema document, or a configuration file supplied can reach a caller
    /// through here, because no runtime value can.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Filtering => "filtering",
            Self::FilteringOnMutations => "filtering on mutations",
            Self::Ordering => "ordering",
            Self::Paging => "paging",
            Self::Comparison(operator) => Self::comparison_name(operator),
            Self::Membership => "the in comparison",
            Self::NullComparison => "null comparison",
            Self::Mutations => "mutations",
            Self::WritesToCollection => "writes to this collection",
            Self::InsertsOnCollection => "inserts on this collection",
            Self::UpdatesOnCollection => "updates on this collection",
            Self::DeletesOnCollection => "deletes on this collection",
        }
    }

    /// Spelt out per operator rather than built with `format!`, which would
    /// give up the `&'static str` guarantee for a saved line.
    const fn comparison_name(operator: ComparisonOperator) -> &'static str {
        match operator {
            ComparisonOperator::Equal => "the equal comparison",
            ComparisonOperator::NotEqual => "the not_equal comparison",
            ComparisonOperator::LessThan => "the less_than comparison",
            ComparisonOperator::LessThanOrEqual => "the less_than_or_equal comparison",
            ComparisonOperator::GreaterThan => "the greater_than comparison",
            ComparisonOperator::GreaterThanOrEqual => "the greater_than_or_equal comparison",
            ComparisonOperator::Contains => "the contains comparison",
        }
    }

    /// Refuses an operation, with nothing further for an operator.
    #[must_use]
    pub const fn refused(self) -> ConnectorError {
        ConnectorError::Unsupported {
            feature: self,
            detail: RefusalDetail::none(),
        }
    }

    /// Refuses an operation and records the physical specifics for an operator.
    ///
    /// The detail never reaches a caller: it is absent from the error's
    /// `Display`, so it travels only to the log line that asks for it by name.
    #[must_use]
    pub fn refused_because(self, detail: impl Into<String>) -> ConnectorError {
        ConnectorError::Unsupported {
            feature: self,
            detail: RefusalDetail::new(detail),
        }
    }
}

impl fmt::Display for UnsupportedFeature {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
