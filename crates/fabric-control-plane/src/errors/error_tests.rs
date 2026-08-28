//! Tests for how a refusal is presented.

use fabric_client_model::{ClientId, DesiredStateError, RealmName};
use http::StatusCode;

use crate::operator::OperatorAuthError;
use crate::repository::RepositoryError;
use crate::ControlPlaneError;

fn client() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

#[test]
fn every_failure_has_its_own_machine_code() {
    let errors = [
        ControlPlaneError::Unauthenticated(OperatorAuthError::Missing),
        ControlPlaneError::UnknownClient(client()),
        ControlPlaneError::InvalidRequest(DesiredStateError::MissingField { field: "spec" }),
        ControlPlaneError::InvalidDesiredState {
            client: client(),
            source: DesiredStateError::MissingField { field: "spec" },
        },
        ControlPlaneError::RevisionRequired,
        ControlPlaneError::RevisionConflict,
        ControlPlaneError::RealmImmutable {
            current: RealmName::try_new("acme").unwrap(),
        },
        ControlPlaneError::RepositoryUnavailable,
        ControlPlaneError::RepositoryDenied,
        ControlPlaneError::RepositoryRejected,
    ];

    let mut codes: Vec<&str> = errors.iter().map(ControlPlaneError::code).collect();
    let total = codes.len();
    codes.sort_unstable();
    codes.dedup();

    assert_eq!(
        codes.len(),
        total,
        "two failures share a code, so a client cannot tell them apart"
    );
}

#[test]
fn a_stale_revision_is_a_conflict_and_a_missing_one_is_a_precondition() {
    // Different problems, different remedies: one means redo your edit, the
    // other means send the header.
    assert_eq!(ControlPlaneError::RevisionConflict.status(), StatusCode::CONFLICT);
    assert_eq!(
        ControlPlaneError::RevisionRequired.status(),
        StatusCode::PRECONDITION_REQUIRED
    );
}

#[test]
fn an_unreadable_stored_document_is_a_server_error_not_a_bad_request() {
    // Nothing the operator sent caused it, and no correction to their request
    // will fix it.
    let error = ControlPlaneError::InvalidDesiredState {
        client: client(),
        source: DesiredStateError::MissingField { field: "spec" },
    };

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn a_refused_platform_credential_is_not_advertised_as_transient() {
    // 503 would invite a retry storm over a secret that will still be wrong.
    assert_eq!(
        ControlPlaneError::RepositoryDenied.status(),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        ControlPlaneError::RepositoryUnavailable.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[test]
fn a_repository_failure_detail_does_not_survive_translation() {
    // The detail may name a branch, a path, or an upstream body. It is logged
    // by the adapter and dropped here.
    let error = ControlPlaneError::from_repository(RepositoryError::Unavailable {
        detail: "github: 500 while reading clients/acme/client.yaml".to_owned(),
    });

    assert!(!error.public_message().contains("clients/acme"));
    assert!(!error.public_message().contains("github"));
}

#[test]
fn a_missing_client_is_reported_as_unknown_rather_than_as_a_repository_failure() {
    let error = ControlPlaneError::from_repository(RepositoryError::NotFound { client: client() });

    assert_eq!(error.status(), StatusCode::NOT_FOUND);
    assert_eq!(error.code(), "unknown_client");
}
