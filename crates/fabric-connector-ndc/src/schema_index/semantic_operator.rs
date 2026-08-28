//! The operator meanings the platform knows how to ask for.

use fabric_connector::{ComparisonOperator, UnsupportedFeature};

use crate::schema_index::OperatorFit;
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
    /// The capability name a caller is told when this meaning is missing.
    ///
    /// The published half of a refusal, so it is drawn from `fabric-connector`'s
    /// closed vocabulary rather than composed here — the type is what stops a
    /// schema's own identifiers reaching an application.
    #[must_use]
    pub(crate) const fn refused_feature(self) -> UnsupportedFeature {
        match self {
            Self::Equal => UnsupportedFeature::Comparison(ComparisonOperator::Equal),
            Self::In => UnsupportedFeature::Membership,
            Self::LessThan => UnsupportedFeature::Comparison(ComparisonOperator::LessThan),
            Self::LessThanOrEqual => UnsupportedFeature::Comparison(ComparisonOperator::LessThanOrEqual),
            Self::GreaterThan => UnsupportedFeature::Comparison(ComparisonOperator::GreaterThan),
            Self::GreaterThanOrEqual => {
                UnsupportedFeature::Comparison(ComparisonOperator::GreaterThanOrEqual)
            }
            Self::Contains => UnsupportedFeature::Comparison(ComparisonOperator::Contains),
        }
    }

    /// A stable name for telemetry and for the operator-only half of a refusal.
    ///
    /// Not what a caller is shown — that is [`Self::refused_feature`]. The
    /// spellings match [`ComparisonOperator::as_str`] so the two read alike.
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::In => "in",
            Self::LessThan => "less_than",
            Self::LessThanOrEqual => "less_than_or_equal",
            Self::GreaterThan => "greater_than",
            Self::GreaterThanOrEqual => "greater_than_or_equal",
            Self::Contains => "contains",
        }
    }

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

    /// Reads the semantic out of a connector's operator definition, and how
    /// exactly it fits.
    ///
    /// Case-insensitive containment is still accepted as containment, but only
    /// as an [`OperatorFit::Widened`] fallback. `scalar-types.md` defines
    /// `contains` and `icontains` as different predicates, so where a scalar
    /// declares both, the exact one must win — see
    /// [`operator_index`](super::operator_index). Where a scalar declares only
    /// the insensitive form the widening still happens, and is safe there for
    /// the same reason as before: tenant scoping is always equality on a
    /// discriminator, never containment.
    ///
    /// Prefix and suffix matching have no neutral counterpart and are not
    /// offered; nor is anything a connector invented.
    pub(super) const fn from_definition(
        definition: &NdcComparisonOperatorDefinition,
    ) -> Option<(Self, OperatorFit)> {
        match definition {
            NdcComparisonOperatorDefinition::Equal => Some((Self::Equal, OperatorFit::Exact)),
            NdcComparisonOperatorDefinition::In => Some((Self::In, OperatorFit::Exact)),
            NdcComparisonOperatorDefinition::LessThan => Some((Self::LessThan, OperatorFit::Exact)),
            NdcComparisonOperatorDefinition::LessThanOrEqual => {
                Some((Self::LessThanOrEqual, OperatorFit::Exact))
            }
            NdcComparisonOperatorDefinition::GreaterThan => Some((Self::GreaterThan, OperatorFit::Exact)),
            NdcComparisonOperatorDefinition::GreaterThanOrEqual => {
                Some((Self::GreaterThanOrEqual, OperatorFit::Exact))
            }
            NdcComparisonOperatorDefinition::Contains => Some((Self::Contains, OperatorFit::Exact)),
            NdcComparisonOperatorDefinition::ContainsInsensitive => {
                Some((Self::Contains, OperatorFit::Widened))
            }
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
