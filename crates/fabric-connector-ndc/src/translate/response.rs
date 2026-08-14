//! NDC responses back to neutral outcomes.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, ConnectorId, FieldName, MutationOutcome, QueryOutcome, Row};
use serde_json::Value;

use crate::wire::{NdcMutationResponse, NdcOperationResult, NdcQueryResponse};

/// Converts a query response into a neutral outcome.
///
/// # Errors
///
/// [`ConnectorError::MalformedResponse`] unless the connector returned exactly
/// one row set. This client never sends variables, and the specification's own
/// schema says that means exactly one comes back — so *both* none and several
/// are malformed. Several is the more dangerous of the two: taking the first
/// would drop the rest with nothing in the outcome to say any were dropped.
pub(crate) fn to_query_outcome(
    connector: &ConnectorId,
    response: &NdcQueryResponse,
) -> Result<QueryOutcome, ConnectorError> {
    let row_set = response.sole().ok_or_else(|| ConnectorError::MalformedResponse {
        connector: connector.clone(),
        detail: format!(
            "the query response contained {} row sets; a request that sends no variables must \
             produce exactly one",
            response.count()
        ),
    })?;

    // An absent `rows` means the query asked for no fields, which is a
    // legitimate empty result rather than an error.
    let rows = row_set
        .rows
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(to_row)
        .collect();

    Ok(QueryOutcome {
        rows,
        // NDC reports counts through aggregates, which the Data API does not
        // use. `None` correctly means "not counted" rather than zero.
        total_count: None,
    })
}

/// Converts one wire row into a neutral row.
///
/// Fields whose names fail platform validation are dropped. That is not
/// silently lossy in practice — the projection is built from validated
/// `FieldName`s, so anything unparseable was not asked for.
fn to_row(fields: &BTreeMap<String, Value>) -> Row {
    Row::from(
        fields
            .iter()
            .filter_map(|(name, value)| Some((FieldName::try_new(name).ok()?, value.clone())))
            .collect::<BTreeMap<_, _>>(),
    )
}

/// Converts a mutation response into a neutral outcome.
///
/// # Interpreting a procedure's return value
///
/// NDC does not define the shape of a procedure's result — it is whatever the
/// procedure declares. There is therefore no universal way to read an
/// affected-row count, so this recognises the conventions in common use and
/// falls back conservatively:
///
/// | Shape | Interpretation |
/// |---|---|
/// | `{"affected_rows": n, ...}` | `n`, plus any `returning` array as rows |
/// | `[...]` | The array is the returned rows; count is its length |
/// | `null` | Nothing affected |
/// | anything else | One row affected, value returned if it is an object |
///
/// # Errors
///
/// [`ConnectorError::MalformedResponse`] if no operation result came back.
pub(crate) fn to_mutation_outcome(
    connector: &ConnectorId,
    response: &NdcMutationResponse,
) -> Result<MutationOutcome, ConnectorError> {
    let result = response
        .operation_results
        .first()
        .ok_or_else(|| ConnectorError::MalformedResponse {
            connector: connector.clone(),
            detail: "the mutation response contained no operation results".to_owned(),
        })?;

    let NdcOperationResult::Procedure { result } = result;

    Ok(interpret(result))
}

/// Reads an outcome out of a procedure's return value.
fn interpret(result: &Value) -> MutationOutcome {
    match result {
        Value::Null => MutationOutcome::affected(0),

        Value::Array(rows) => {
            let returned = rows.iter().filter_map(as_row).collect::<Vec<_>>();
            MutationOutcome::affected(rows.len() as u64).with_rows(returned)
        }

        Value::Object(fields) => {
            let affected = fields.get("affected_rows").and_then(Value::as_u64);

            let returned = fields
                .get("returning")
                .and_then(Value::as_array)
                .map(|rows| rows.iter().filter_map(as_row).collect::<Vec<_>>())
                .unwrap_or_default();

            match affected {
                Some(count) => MutationOutcome::affected(count).with_rows(returned),
                // An object with no count is most likely the written row itself.
                None => MutationOutcome::affected(1).with_rows(as_row(result).into_iter().collect()),
            }
        }

        _ => MutationOutcome::affected(1),
    }
}

/// Reads a JSON object as a neutral row.
fn as_row(value: &Value) -> Option<Row> {
    let object = value.as_object()?;

    Some(Row::from(
        object
            .iter()
            .filter_map(|(name, value)| Some((FieldName::try_new(name).ok()?, value.clone())))
            .collect::<BTreeMap<_, _>>(),
    ))
}
