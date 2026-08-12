//! NDC responses back to neutral outcomes.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, ConnectorId, FieldName, MutationOutcome, QueryOutcome, Row};
use serde_json::Value;

use crate::wire::{NdcMutationResponse, NdcOperationResult, NdcQueryResponse};

/// Converts a query response into a neutral outcome.
///
/// # Errors
///
/// [`ConnectorError::MalformedResponse`] if the connector returned no row set
/// at all. We never send variables, so a conforming connector must return
/// exactly one.
pub(crate) fn to_query_outcome(
    connector: &ConnectorId,
    response: &NdcQueryResponse,
) -> Result<QueryOutcome, ConnectorError> {
    let row_set = response
        .first()
        .ok_or_else(|| ConnectorError::MalformedResponse {
            connector: connector.clone(),
            detail: "the query response contained no row sets".to_owned(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn connector() -> ConnectorId {
        ConnectorId::try_new("postgres").unwrap()
    }

    fn query_response(json: &str) -> NdcQueryResponse {
        serde_json::from_str(json).unwrap()
    }

    fn mutation_response(json: &str) -> NdcMutationResponse {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn reads_rows_out_of_a_row_set() {
        let response = query_response(r#"[{"rows":[{"id":1,"name":"Alice"}]}]"#);

        let outcome = to_query_outcome(&connector(), &response).unwrap();

        assert_eq!(outcome.len(), 1);
        assert_eq!(
            outcome
                .rows
                .first()
                .unwrap()
                .get(&FieldName::try_new("name").unwrap()),
            Some(&Value::String("Alice".to_owned()))
        );
    }

    #[test]
    fn an_absent_rows_member_is_an_empty_result_not_an_error() {
        let outcome = to_query_outcome(&connector(), &query_response("[{}]")).unwrap();

        assert!(outcome.is_empty());
    }

    #[test]
    fn total_count_is_not_counted_rather_than_zero() {
        let outcome = to_query_outcome(&connector(), &query_response(r#"[{"rows":[]}]"#)).unwrap();

        assert_eq!(outcome.total_count, None);
    }

    #[test]
    fn a_response_with_no_row_set_is_malformed() {
        let error = to_query_outcome(&connector(), &query_response("[]")).unwrap_err();

        assert!(matches!(error, ConnectorError::MalformedResponse { .. }));
    }

    #[test]
    fn reads_an_affected_row_count_and_returned_rows() {
        let response = mutation_response(
            r#"{"operation_results":[{"type":"procedure","result":{"affected_rows":2,"returning":[{"id":1},{"id":2}]}}]}"#,
        );

        let outcome = to_mutation_outcome(&connector(), &response).unwrap();

        assert_eq!(outcome.affected_rows, 2);
        assert_eq!(outcome.returned_rows.len(), 2);
    }

    #[test]
    fn an_array_result_counts_its_entries() {
        let response =
            mutation_response(r#"{"operation_results":[{"type":"procedure","result":[{"id":1},{"id":2}]}]}"#);

        let outcome = to_mutation_outcome(&connector(), &response).unwrap();

        assert_eq!(outcome.affected_rows, 2);
        assert_eq!(outcome.returned_rows.len(), 2);
    }

    #[test]
    fn a_null_result_means_nothing_was_affected() {
        let response = mutation_response(r#"{"operation_results":[{"type":"procedure","result":null}]}"#);

        assert_eq!(
            to_mutation_outcome(&connector(), &response)
                .unwrap()
                .affected_rows,
            0
        );
    }

    #[test]
    fn a_bare_object_result_is_treated_as_one_written_row() {
        let response = mutation_response(
            r#"{"operation_results":[{"type":"procedure","result":{"id":7,"name":"Alice"}}]}"#,
        );

        let outcome = to_mutation_outcome(&connector(), &response).unwrap();

        assert_eq!(outcome.affected_rows, 1);
        assert_eq!(outcome.returned_rows.len(), 1);
    }

    #[test]
    fn a_response_with_no_operation_results_is_malformed() {
        let response = mutation_response(r#"{"operation_results":[]}"#);

        assert!(matches!(
            to_mutation_outcome(&connector(), &response).unwrap_err(),
            ConnectorError::MalformedResponse { .. }
        ));
    }
}
