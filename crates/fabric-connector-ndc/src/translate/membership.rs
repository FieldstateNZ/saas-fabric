//! Set membership, and what to do when the connector declares no `in`.

use fabric_connector::{CollectionName, ConnectorError, FieldName};
use serde_json::Value;

use crate::schema_index::SemanticOperator;
use crate::translate::expression::comparison;
use crate::wire::{NdcComparisonTarget, NdcComparisonValue, NdcExpression};
use crate::SchemaIndex;

/// Translates set membership, falling back to a disjunction of equalities.
///
/// The fallback is not a degradation: `x IN (a, b)` and `x = a OR x = b` are the
/// same predicate. It simply lets a connector that never declared an `in`
/// operator still serve the query, which is worth doing because `in` is
/// commonly omitted.
///
/// # Errors
///
/// - [`ConnectorError::InvalidOperation`] for an empty value list — see
///   [`reject_empty`].
/// - [`ConnectorError::Unsupported`] when the field's type declares neither an
///   `in` nor an equality operator.
pub(super) fn translate_membership(
    collection: &CollectionName,
    field: &FieldName,
    values: &[Value],
    index: &SchemaIndex,
) -> Result<NdcExpression, ConnectorError> {
    reject_empty(values)?;

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

    Ok(NdcExpression::Or {
        expressions: alternatives,
    })
}

/// Refuses membership of the empty set.
///
/// The emitted `or[]` was safe in the narrowing direction — it matches nothing,
/// which is what the predicate means — but it reached the wire without a single
/// schema lookup, so it happily filtered on a column the connector does not
/// have. That contradicts the translation layer's own fail-closed rule, and a
/// rule with a silent exception is the kind that erodes.
///
/// It was not the *only* such path, as this comment previously claimed:
/// `Filter::IsNull` had the same hole, and closing one while leaving the other
/// open is how a rule ends up meaning less than it says. Both now check —
/// see [`null_check`](super::expression) for the other half.
///
/// It is also a caller mistake worth naming rather than satisfying: nothing
/// upstream constructs an empty `In` deliberately, so one arriving here means
/// a request was built wrong.
///
/// # Errors
///
/// [`ConnectorError::InvalidOperation`], which the Data API masks to a generic
/// 400. The reason reaches an operator through `logging::operation_refused`.
fn reject_empty(values: &[Value]) -> Result<(), ConnectorError> {
    if values.is_empty() {
        return Err(ConnectorError::InvalidOperation(
            "a membership test reached the connector with no values to match".to_owned(),
        ));
    }

    Ok(())
}
