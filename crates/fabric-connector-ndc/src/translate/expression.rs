//! Neutral [`Filter`] to NDC predicate.

use fabric_connector::{CollectionName, ComparisonOperator, ConnectorError, FieldName, Filter};
use serde_json::Value;

use crate::schema_index::SemanticOperator;
use crate::translate::membership::translate_membership;
use crate::wire::{NdcComparisonTarget, NdcComparisonValue, NdcExpression, NdcUnaryOperator};
use crate::SchemaIndex;

/// Translates a neutral predicate into an NDC predicate.
///
/// # Errors
///
/// [`ConnectorError::Unsupported`] when the connector's schema declares no
/// operator with the required meaning for the field's type. The operation is
/// refused rather than approximated.
///
/// [`ConnectorError::InvalidOperation`] for a membership test with no values.
pub(crate) fn to_expression(
    collection: &CollectionName,
    filter: &Filter,
    index: &SchemaIndex,
) -> Result<NdcExpression, ConnectorError> {
    match filter {
        Filter::And { clauses } => Ok(NdcExpression::And {
            expressions: translate_all(collection, clauses, index)?,
        }),

        Filter::Or { clauses } => Ok(NdcExpression::Or {
            expressions: translate_all(collection, clauses, index)?,
        }),

        Filter::Not { clause } => Ok(NdcExpression::Not {
            expression: Box::new(to_expression(collection, clause, index)?),
        }),

        Filter::IsNull { field } => Ok(NdcExpression::UnaryComparisonOperator {
            column: NdcComparisonTarget::column(field.as_str()),
            operator: NdcUnaryOperator::IsNull,
        }),

        Filter::Compare {
            field,
            operator: ComparisonOperator::NotEqual,
            value,
        } => {
            // NDC has no not-equal semantic, so inequality is a negated
            // equality. Every connector that can compare for equality can
            // therefore serve it.
            let equality = comparison(collection, field, SemanticOperator::Equal, value.clone(), index)?;

            Ok(NdcExpression::Not {
                expression: Box::new(equality),
            })
        }

        Filter::Compare {
            field,
            operator,
            value,
        } => comparison(
            collection,
            field,
            SemanticOperator::for_neutral(*operator),
            value.clone(),
            index,
        ),

        Filter::In { field, values } => translate_membership(collection, field, values, index),
    }
}

/// Translates every clause of a compound predicate.
fn translate_all(
    collection: &CollectionName,
    clauses: &[Filter],
    index: &SchemaIndex,
) -> Result<Vec<NdcExpression>, ConnectorError> {
    clauses
        .iter()
        .map(|clause| to_expression(collection, clause, index))
        .collect()
}

/// Builds a binary comparison, resolving the connector's operator name.
///
/// The refusal names the *semantic* the caller needed, not the column it was
/// needed for — and cannot name the column, because
/// [`UnsupportedFeature`](fabric_connector::UnsupportedFeature) has nowhere to
/// put one. That matters most here: by the time this runs the predicate may
/// include the tenant discriminator `for_target` conjoined, so the column in
/// hand can be the isolation column itself. It goes to the
/// [`RefusalDetail`](fabric_connector::RefusalDetail) instead, which reaches an
/// operator's log and nothing else.
///
/// An inequality arrives here as [`SemanticOperator::Equal`], because that is
/// genuinely what is missing: it is negated afterwards, so a connector with no
/// equality operator cannot serve either form.
pub(super) fn comparison(
    collection: &CollectionName,
    field: &FieldName,
    semantic: SemanticOperator,
    value: Value,
    index: &SchemaIndex,
) -> Result<NdcExpression, ConnectorError> {
    let operator = index.operator_name(collection, field, semantic).ok_or_else(|| {
        semantic.refused_feature().refused_because(format!(
            "{collection}.{field} has no {} operator in the connector schema",
            semantic.as_str()
        ))
    })?;

    Ok(NdcExpression::BinaryComparisonOperator {
        column: NdcComparisonTarget::column(field.as_str()),
        operator: operator.to_owned(),
        value: NdcComparisonValue::scalar(value),
    })
}
