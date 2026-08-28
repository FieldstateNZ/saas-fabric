//! Tests for how the Git host's failures are reported upward.

use fabric_client_model::ClientId;
use fabric_control_plane::RepositoryError;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::StatusCode;

use super::errors::status_failure;

fn client() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

fn headers(remaining: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(remaining) = remaining {
        if let Ok(value) = HeaderValue::from_str(remaining) {
            headers.insert("x-ratelimit-remaining", value);
        }
    }
    headers
}

#[test]
fn a_missing_file_for_a_named_client_is_reported_as_that_client_being_absent() {
    let error = status_failure(
        "reading a client",
        StatusCode::NOT_FOUND,
        &headers(None),
        Some(&client()),
    );

    assert!(matches!(error, RepositoryError::NotFound { .. }));
}

#[test]
fn a_missing_directory_is_not_reported_as_a_client_being_absent() {
    // The listing path has no client to name, and reporting "no such client"
    // for a repository whose layout has moved would send an operator looking
    // for the wrong thing.
    let error = status_failure("listing clients", StatusCode::NOT_FOUND, &headers(None), None);

    assert!(matches!(error, RepositoryError::Unavailable { .. }));
}

#[test]
fn a_rate_limit_is_transient_and_a_refused_token_is_not() {
    // Both arrive as 403. Reporting a rate limit as a refused credential sends
    // an operator to rotate a secret that is perfectly fine.
    let limited = status_failure(
        "listing clients",
        StatusCode::FORBIDDEN,
        &headers(Some("0")),
        None,
    );
    let refused = status_failure(
        "listing clients",
        StatusCode::FORBIDDEN,
        &headers(Some("4999")),
        None,
    );

    assert!(matches!(limited, RepositoryError::Unavailable { .. }));
    assert!(matches!(refused, RepositoryError::NotPermitted));
}

#[test]
fn both_precondition_statuses_mean_the_same_thing_to_an_operator() {
    for status in [StatusCode::CONFLICT, StatusCode::UNPROCESSABLE_ENTITY] {
        assert!(matches!(
            status_failure("writing a client", status, &headers(None), None),
            RepositoryError::Conflict
        ));
    }
}

#[test]
fn an_unexpected_client_error_is_refused_rather_than_advertised_as_transient() {
    let error = status_failure("writing a client", StatusCode::BAD_REQUEST, &headers(None), None);

    assert!(matches!(error, RepositoryError::Rejected { .. }));
}

#[test]
fn a_server_error_is_transient() {
    let error = status_failure("writing a client", StatusCode::BAD_GATEWAY, &headers(None), None);

    assert!(matches!(error, RepositoryError::Unavailable { .. }));
}

#[test]
fn the_message_never_carries_an_upstream_body() {
    // The detail is composed from the operation and the status, and nothing
    // the host said reaches it.
    let error = status_failure("writing a client", StatusCode::BAD_REQUEST, &headers(None), None);

    assert!(error.to_string().contains("writing a client"));
    assert!(error.to_string().contains("400"));
}
