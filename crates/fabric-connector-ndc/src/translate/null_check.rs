//! Testing a column for null, and why that still needs the schema.

use fabric_connector::{CollectionName, ConnectorError, FieldName, UnsupportedFeature};

use crate::wire::{NdcComparisonTarget, NdcExpression, NdcUnaryOperator};
use crate::SchemaIndex;

/// Builds a null test, after checking the connector declares the column.
///
/// `is_null` needs no *operator* lookup — it is a core NDC unary operator with
/// no capability key, which is why
/// [`to_capabilities`](super::to_capabilities) reports `null_checks`
/// unconditionally. It does still need the column to exist. Without this
/// check it was a path through the translator that reached the wire having
/// consulted the schema not at all, so a predicate could name a column the
/// connector has never heard of; the refusal would then come back as an opaque
/// 4xx from the connector rather than as this crate's own, and the two are not
/// interchangeable — one says the caller asked for something unsupported, the
/// other says the connector is having a bad day.
///
/// # Existence, not comparability
///
/// Deliberately the weaker check. A null test is well defined on an array
/// column, which has no scalar type and therefore no entry in the operator
/// index at all — see [`NdcType::named`](crate::wire::NdcType::named). Asking
/// for an operator here would refuse a predicate that is perfectly
/// expressible, which is the opposite error from the one this module exists to
/// prevent but an error all the same.
///
/// # Errors
///
/// [`ConnectorError::Unsupported`] naming
/// [`UnsupportedFeature::NullComparison`], with the column in the refusal
/// detail where only an operator's log will see it.
pub(super) fn translate_null_check(
    collection: &CollectionName,
    field: &FieldName,
    index: &SchemaIndex,
) -> Result<NdcExpression, ConnectorError> {
    if !index.has_field(collection, field) {
        return Err(UnsupportedFeature::NullComparison.refused_because(format!(
            "{collection}.{field} is not a column in the connector schema"
        )));
    }

    Ok(NdcExpression::UnaryComparisonOperator {
        column: NdcComparisonTarget::column(field.as_str()),
        operator: NdcUnaryOperator::IsNull,
    })
}
