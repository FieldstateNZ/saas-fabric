//! The comparison operators a backend may support.

/// A binary comparison in a [`Filter`](crate::Filter).
///
/// A connector declares which of these it supports through
/// [`ConnectorCapabilities`](crate::ConnectorCapabilities). An operator a
/// backend cannot express causes the operation to be **refused**, never
/// approximated — see the type-level docs on `ConnectorCapabilities` for why
/// that matters more here than in a single-tenant system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    /// Equal to.
    Equal,
    /// Not equal to.
    NotEqual,
    /// Strictly less than.
    LessThan,
    /// Less than or equal to.
    LessThanOrEqual,
    /// Strictly greater than.
    GreaterThan,
    /// Greater than or equal to.
    GreaterThanOrEqual,
    /// The field's value contains the given value as a substring.
    ///
    /// Deliberately *containment*, not SQL `LIKE`. A pattern operator would
    /// have to define whose pattern syntax applies — and `%` means different
    /// things across backends, so the platform would either pick a dialect and
    /// leak it, or translate patterns and get it subtly wrong. Containment has
    /// one meaning everywhere.
    Contains,
}

impl ComparisonOperator {
    /// A stable name for telemetry and for wire encoding.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::NotEqual => "not_equal",
            Self::LessThan => "less_than",
            Self::LessThanOrEqual => "less_than_or_equal",
            Self::GreaterThan => "greater_than",
            Self::GreaterThanOrEqual => "greater_than_or_equal",
            Self::Contains => "contains",
        }
    }
}
