//! Tests for response.

use super::response::*;
use crate::wire::{NdcMutationResponse, NdcQueryResponse};
use fabric_connector::{ConnectorError, ConnectorId, FieldName};
use serde_json::Value;

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
    let response =
        mutation_response(r#"{"operation_results":[{"type":"procedure","result":{"id":7,"name":"Alice"}}]}"#);

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
