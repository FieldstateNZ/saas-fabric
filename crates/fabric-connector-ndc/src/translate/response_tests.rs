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

/// Reads a real `ndc-postgres` v3.1.0 document, checked in under
/// `tests/fixtures/` — see the README there for how it was captured.
fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/ndc-postgres-v3.1.0/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
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
fn a_response_with_more_than_one_row_set_is_malformed_not_truncated() {
    // Multiple row sets come back only for a query using variables, and this
    // client never sends any. Taking the first would have dropped the rest
    // with nothing in the outcome to say so -- a `200` that quietly answers a
    // different question than the one asked.
    let response = query_response(r#"[{"rows":[{"id":1}]},{"rows":[{"id":2}]}]"#);

    let error = to_query_outcome(&connector(), &response).unwrap_err();

    let ConnectorError::MalformedResponse { detail, .. } = &error else {
        panic!("expected MalformedResponse, got {error:?}");
    };
    assert!(detail.contains('2'), "{detail}");
    assert!(detail.contains("exactly one"), "{detail}");
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

/// F1: both observed responses to a `fields`-carrying insert — the one asking
/// `affected_rows` and `returning` together, and the one asking `affected_rows`
/// alone — read the same count and only the first carries a returned row.
/// `docs/decisions/0004-*.md`'s addendum and
/// `tests/fixtures/ndc-postgres-v3.1.0/README.md` record how each was taken.
/// This one, `mutation-insert-ok.json`, is a real capture -- `tee`d straight
/// from `curl`, spacing and all; the next test's fixture is not (see its own
/// doc comment).
#[test]
fn the_observed_insert_response_with_returning_reads_the_row_back() {
    let response = mutation_response(&fixture("mutation-insert-ok.json"));

    let outcome = to_mutation_outcome(&connector(), &response).unwrap();

    assert_eq!(outcome.affected_rows, 1);
    assert_eq!(outcome.returned_rows.len(), 1);
    assert_eq!(
        outcome.returned_rows[0].get(&FieldName::try_new("title").unwrap()),
        Some(&Value::String("Inserted".to_owned()))
    );
}

/// Unlike the previous test's fixture, `mutation-insert-affected-only.json`
/// is a **reconstruction**, not a saved capture: the probe that produced it
/// was quoted in the planning document rather than `tee`d to a file at the
/// time (`tests/fixtures/ndc-postgres-v3.1.0/README.md`'s "Two captures
/// reconstructed from the plan's record" section). It is transcribed exactly
/// as the plan quoted it, immediately after observing the real run, but it
/// did not come from a saved byte-for-byte capture the way the test above's
/// fixture did.
#[test]
fn the_observed_insert_response_with_affected_rows_alone_reads_the_count_and_no_rows() {
    let response = mutation_response(&fixture("mutation-insert-affected-only.json"));

    let outcome = to_mutation_outcome(&connector(), &response).unwrap();

    assert_eq!(outcome.affected_rows, 1);
    assert!(outcome.returned_rows.is_empty());
}

/// The delete scoped to the other tenant: `affected_rows: 0`, because the
/// `pre_check` predicate matched nothing — the isolation guarantee holding on
/// the write path, not a parsing detail. Like
/// `mutation-insert-affected-only.json` above, `mutation-delete-other-tenant.json`
/// is a reconstruction from the plan's quoted record, not a saved capture --
/// see that test's doc comment and the fixture README.
#[test]
fn the_observed_delete_scoped_to_another_tenant_reads_zero_affected() {
    let response = mutation_response(&fixture("mutation-delete-other-tenant.json"));

    let outcome = to_mutation_outcome(&connector(), &response).unwrap();

    assert_eq!(outcome.affected_rows, 0);
}

#[test]
fn a_response_with_no_operation_results_is_malformed() {
    let response = mutation_response(r#"{"operation_results":[]}"#);

    assert!(matches!(
        to_mutation_outcome(&connector(), &response).unwrap_err(),
        ConnectorError::MalformedResponse { .. }
    ));
}
