//! The operator meanings the platform knows how to ask for.

use fabric_connector::ComparisonOperator;

use crate::wire::NdcComparisonOperatorDefinition;

/// A comparison *meaning*, independent of what any connector calls it.
///
/// Note the absence of "not equal": NDC has no such semantic. Inequality is
/// expressed as a negated equality, which every connector supporting `Equal`
/// can therefore do. That translation happens in `translate::expression`, not
/// here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticOperator {
    /// Equality — also the basis for inequality, via negation.
    Equal,
    /// Membership in a list.
    In,
    /// Strictly less than.
    LessThan,
    /// Less than or equal.
    LessThanOrEqual,
    /// Strictly greater than.
    GreaterThan,
    /// Greater than or equal.
    GreaterThanOrEqual,
    /// Substring containment.
    Contains,
}

impl SemanticOperator {
    /// The semantic a neutral operator needs from the connector.
    #[must_use]
    pub const fn for_neutral(operator: ComparisonOperator) -> Self {
        match operator {
            // Inequality is a negated equality, so it needs the same operator.
            ComparisonOperator::Equal | ComparisonOperator::NotEqual => Self::Equal,
            ComparisonOperator::LessThan => Self::LessThan,
            ComparisonOperator::LessThanOrEqual => Self::LessThanOrEqual,
            ComparisonOperator::GreaterThan => Self::GreaterThan,
            ComparisonOperator::GreaterThanOrEqual => Self::GreaterThanOrEqual,
            ComparisonOperator::Contains => Self::Contains,
        }
    }

    /// Reads the semantic out of a connector's operator definition.
    ///
    /// Case-insensitive containment is accepted as containment — a deliberate
    /// widening, safe because tenant scoping is always equality on a
    /// discriminator, never containment. Prefix and suffix matching have no
    /// neutral counterpart and are not offered; nor is anything a connector
    /// invented.
    pub(super) const fn from_definition(definition: &NdcComparisonOperatorDefinition) -> Option<Self> {
        match definition {
            NdcComparisonOperatorDefinition::Equal => Some(Self::Equal),
            NdcComparisonOperatorDefinition::In => Some(Self::In),
            NdcComparisonOperatorDefinition::LessThan => Some(Self::LessThan),
            NdcComparisonOperatorDefinition::LessThanOrEqual => Some(Self::LessThanOrEqual),
            NdcComparisonOperatorDefinition::GreaterThan => Some(Self::GreaterThan),
            NdcComparisonOperatorDefinition::GreaterThanOrEqual => Some(Self::GreaterThanOrEqual),
            NdcComparisonOperatorDefinition::Contains
            | NdcComparisonOperatorDefinition::ContainsInsensitive => Some(Self::Contains),
            NdcComparisonOperatorDefinition::StartsWith
            | NdcComparisonOperatorDefinition::StartsWithInsensitive
            | NdcComparisonOperatorDefinition::EndsWith
            | NdcComparisonOperatorDefinition::EndsWithInsensitive
            | NdcComparisonOperatorDefinition::Custom => None,
        }
    }

    /// Every neutral operator the platform might ask for.
    pub(super) const fn neutral_candidates() -> [ComparisonOperator; 7] {
        [
            ComparisonOperator::Equal,
            ComparisonOperator::NotEqual,
            ComparisonOperator::LessThan,
            ComparisonOperator::LessThanOrEqual,
            ComparisonOperator::GreaterThan,
            ComparisonOperator::GreaterThanOrEqual,
            ComparisonOperator::Contains,
        ]
    }
}
