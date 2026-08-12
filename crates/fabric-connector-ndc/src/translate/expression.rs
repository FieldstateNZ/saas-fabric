//! Neutral [`Filter`] to NDC predicate.

use fabric_connector::{CollectionName, ComparisonOperator, ConnectorError, FieldName, Filter};
use serde_json::Value;

use crate::schema_index::SemanticOperator;
use crate::wire::{NdcComparisonTarget, NdcComparisonValue, NdcExpression, NdcUnaryOperator};
use crate::SchemaIndex;

/// Translates a neutral predicate into an NDC predicate.
///
/// # Errors
///
/// [`ConnectorError::Unsupported`] when the connector's schema declares no
/// operator with the required meaning for the field's type. The operation is
/// refused rather than approximated.
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
fn comparison(
    collection: &CollectionName,
    field: &FieldName,
    semantic: SemanticOperator,
    value: Value,
    index: &SchemaIndex,
) -> Result<NdcExpression, ConnectorError> {
    let operator =
        index
            .operator_name(collection, field, semantic)
            .ok_or_else(|| ConnectorError::Unsupported {
                feature: format!("comparing {collection}.{field} with a {semantic:?} operator"),
            })?;

    Ok(NdcExpression::BinaryComparisonOperator {
        column: NdcComparisonTarget::column(field.as_str()),
        operator: operator.to_owned(),
        value: NdcComparisonValue::scalar(value),
    })
}

/// Translates set membership, falling back to a disjunction of equalities.
///
/// The fallback is not a degradation: `x IN (a, b)` and `x = a OR x = b` are the
/// same predicate. It simply lets a connector that never declared an `in`
/// operator still serve the query, which is worth doing because `in` is
/// commonly omitted.
fn translate_membership(
    collection: &CollectionName,
    field: &FieldName,
    values: &[Value],
    index: &SchemaIndex,
) -> Result<NdcExpression, ConnectorError> {
    if let Some(operator) = index.operator_name(collection, field, SemanticOperator::In) {
        return Ok(NdcExpression::BinaryComparisonOperator {
            column: NdcComparisonTarget::column(field.as_str()),
            operator: operator.to_owned(),
            value: NdcComparisonValue::scalar(Value::Array(values.to_vec())),
        });
    }

    let alternatives = values
        .iter()
        .map(|value| comparison(collection, field, SemanticOperator::Equal, value.clone(), index))
        .collect::<Result<Vec<_>, _>>()?;

    // An empty `Or` matches nothing, which is exactly what membership of an
    // empty set means.
    Ok(NdcExpression::Or {
        expressions: alternatives,
    })
}
