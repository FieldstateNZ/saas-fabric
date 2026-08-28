//! Tests for how Keycloak's failures are reported upward.

use fabric_reconciliation::ProviderError;
use reqwest::StatusCode;

use super::errors::status_failure;

#[test]
fn a_refused_credential_is_not_reported_as_the_provider_being_unwell() {
    // Otherwise a misconfigured secret looks like a restarting Keycloak, and
    // an operator waits for it to resolve itself.
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        assert_eq!(
            status_failure("create realm", status),
            ProviderError::NotPermitted
        );
    }
}

#[test]
fn a_server_error_is_transient_and_a_client_error_is_not() {
    assert!(status_failure("create realm", StatusCode::BAD_GATEWAY).is_transient());
    assert!(!status_failure("create realm", StatusCode::BAD_REQUEST).is_transient());
}

#[test]
fn the_message_names_the_operation_and_the_status_and_nothing_else() {
    let error = status_failure("create realm role", StatusCode::BAD_REQUEST);
    let message = error.to_string();

    assert!(message.contains("create realm role"));
    assert!(message.contains("400"));
}
