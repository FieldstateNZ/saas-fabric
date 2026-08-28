//! What a connector can answer with, and what we make of it.

use fabric_connector::{ConnectorError, ConnectorId};

use crate::client::response_decoding::decode_body;
use crate::wire::NdcCapabilitiesResponse;

fn connector() -> ConnectorId {
    ConnectorId::try_new("postgres").unwrap()
}

#[test]
fn a_successful_status_with_valid_json_decodes() {
    let body = br#"{"version":"0.2.13","capabilities":{}}"#;

    let response: NdcCapabilitiesResponse = decode_body(&connector(), reqwest::StatusCode::OK, body).unwrap();

    assert_eq!(response.version, "0.2.13");
}

#[test]
fn a_non_json_capabilities_body_is_a_clear_malformed_error() {
    let body = b"<html>not json</html>";

    let error =
        decode_body::<NdcCapabilitiesResponse>(&connector(), reqwest::StatusCode::OK, body).unwrap_err();

    assert!(matches!(error, ConnectorError::MalformedResponse { .. }));
}

#[test]
fn a_capabilities_body_missing_the_version_field_is_a_clear_malformed_error() {
    // `version` is required, not `#[serde(default)]` — a connector that
    // omits it is exactly the case this must not pass through silently.
    let body = br#"{"capabilities":{}}"#;

    let error =
        decode_body::<NdcCapabilitiesResponse>(&connector(), reqwest::StatusCode::OK, body).unwrap_err();

    assert!(matches!(error, ConnectorError::MalformedResponse { .. }));
}

#[test]
fn a_non_success_status_is_a_rejection_not_a_malformed_response() {
    let error = decode_body::<NdcCapabilitiesResponse>(
        &connector(),
        reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        b"{}",
    )
    .unwrap_err();

    assert!(matches!(error, ConnectorError::Rejected { .. }));
}

#[test]
fn a_non_success_status_carrying_valid_json_is_still_a_rejection() {
    // A connector reporting a database error may well answer with a
    // perfectly well-formed JSON error body. Parsing succeeding is not the
    // question; the status is.
    let error = decode_body::<NdcCapabilitiesResponse>(
        &connector(),
        reqwest::StatusCode::BAD_REQUEST,
        br#"{"message":"relation does not exist"}"#,
    )
    .unwrap_err();

    assert!(matches!(error, ConnectorError::Rejected { .. }));
}
