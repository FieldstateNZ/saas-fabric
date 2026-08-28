//! Building the arguments a mutation procedure expects.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, Filter, MutationSpec, Row, UnsupportedFeature};
use serde_json::Value;

use crate::config::ProcedureBinding;
use crate::translate::to_expression;
use crate::SchemaIndex;

/// Requires a procedure mapping for an operation.
///
/// The two name-shaped parameters go to different audiences and the types say
/// which is which: `feature` is what the caller is told and can hold nothing
/// physical, while `operation` and `collection` only ever reach the log.
///
/// # Errors
///
/// [`ConnectorError::Unsupported`] when the collection has no mapping for this
/// verb. The platform does not guess procedure names — unwise for an insert,
/// indefensible for a delete.
pub(super) fn require<'a>(
    binding: Option<&'a ProcedureBinding>,
    feature: UnsupportedFeature,
    operation: &str,
    collection: &str,
) -> Result<&'a ProcedureBinding, ConnectorError> {
    binding.ok_or_else(|| {
        feature.refused_because(format!("no {operation} procedure is mapped for {collection}"))
    })
}

/// Places a payload under its configured argument name.
///
/// # Errors
///
/// [`ConnectorError::InvalidOperation`] if the mapping declares no payload
/// argument to put the rows in.
pub(super) fn payload(
    binding: &ProcedureBinding,
    value: Value,
) -> Result<BTreeMap<String, Value>, ConnectorError> {
    let name = binding.payload_argument.as_ref().ok_or_else(|| {
        ConnectorError::InvalidOperation(format!(
            "procedure {} needs a payload_argument to carry the rows to write",
            binding.procedure
        ))
    })?;

    Ok(BTreeMap::from([(name.clone(), value)]))
}

/// Builds the arguments for an insert.
///
/// # Errors
///
/// As [`payload`].
pub(super) fn for_insert(
    binding: &ProcedureBinding,
    rows: &[Row],
) -> Result<BTreeMap<String, Value>, ConnectorError> {
    payload(binding, Value::Array(rows.iter().map(row_to_json).collect()))
}

/// Places the predicate under its configured argument name.
///
/// The predicate is sent as an NDC expression, which is what a procedure
/// argument of NDC's `predicate` type expects.
///
/// # Errors
///
/// A missing `filter_argument` is refused rather than skipped: skipping it
/// would drop the tenant scoping `for_target` just added, turning a
/// tenant-scoped delete into a table-wide one. Configuration validation catches
/// this at startup too; this is the second line of defence.
///
/// A mutation arriving with no predicate at all is refused for the same reason.
pub(super) fn add_predicate(
    arguments: &mut BTreeMap<String, Value>,
    binding: &ProcedureBinding,
    filter: Option<&Filter>,
    spec: &MutationSpec,
    index: &SchemaIndex,
) -> Result<(), ConnectorError> {
    let Some(name) = binding.filter_argument.as_ref() else {
        return Err(ConnectorError::InvalidOperation(format!(
            "procedure {} has no filter_argument, so the tenant predicate could not be sent",
            binding.procedure
        )));
    };

    let Some(filter) = filter else {
        return Err(ConnectorError::InvalidOperation(format!(
            "a {} on {} reached the connector with no predicate",
            spec.operation_name(),
            spec.collection()
        )));
    };

    let expression = to_expression(spec.collection(), filter, index)?;

    let encoded = serde_json::to_value(&expression).map_err(|error| {
        ConnectorError::InvalidOperation(format!("could not encode the predicate: {error}"))
    })?;

    arguments.insert(name.clone(), encoded);

    Ok(())
}

/// Converts a neutral row to a JSON object.
pub(super) fn row_to_json(row: &Row) -> Value {
    Value::Object(
        row.as_map()
            .iter()
            .map(|(field, value)| (field.to_string(), value.clone()))
            .collect(),
    )
}
