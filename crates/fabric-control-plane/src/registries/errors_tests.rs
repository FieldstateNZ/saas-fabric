//! Each registry failure's status and code: its own, never the platform's.

use http::StatusCode;

use crate::{ControlPlaneError, Readability, RegistryFailure};
use fabric_platform_management::RegistryError;

#[test]
fn every_registry_failure_answers_its_own_status_and_code() {
    for (failure, status, code) in [
        (
            RegistryFailure::NotFound {
                host: "ghcr.io".into(),
            },
            StatusCode::NOT_FOUND,
            "registry_not_found",
        ),
        (
            RegistryFailure::Exists {
                host: "ghcr.io".into(),
            },
            StatusCode::CONFLICT,
            "registry_exists",
        ),
        (
            RegistryFailure::Invalid("a rule".into()),
            StatusCode::BAD_REQUEST,
            "registry_invalid",
        ),
        (
            RegistryFailure::EndpointDiffers {
                host: "ghcr.io".into(),
            },
            StatusCode::CONFLICT,
            "registry_endpoint_differs",
        ),
        (
            RegistryFailure::NotProven("x".into()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "registry_not_proven",
        ),
        (
            RegistryFailure::RepositoryNotReadable {
                repository: "ghcr.io/a/b".into(),
            },
            StatusCode::UNPROCESSABLE_ENTITY,
            "repository_not_readable",
        ),
        (
            RegistryFailure::Refused("x".into()),
            StatusCode::BAD_GATEWAY,
            "registry_refused",
        ),
        (
            RegistryFailure::Unavailable("x".into()),
            StatusCode::SERVICE_UNAVAILABLE,
            "registry_unavailable",
        ),
        (
            RegistryFailure::StoreUnavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "registries_unavailable",
        ),
        (
            RegistryFailure::StoreInvalid,
            StatusCode::INTERNAL_SERVER_ERROR,
            "registries_invalid",
        ),
    ] {
        let error = ControlPlaneError::from(failure);
        assert_eq!(error.status(), status, "{code}");
        assert_eq!(error.code(), code);
        assert!(!error.code().starts_with("platform"));
    }
}

#[test]
fn a_refused_credential_is_a_bad_gateway_and_never_a_retryable_outage() {
    let failure = RegistryFailure::proving_registry(RegistryError::Denied {
        detail: "the realm refused".into(),
    });
    assert_eq!(ControlPlaneError::from(failure).status(), StatusCode::BAD_GATEWAY);
}

#[test]
fn not_readable_is_one_message() {
    let failure = RegistryFailure::readable::<()>(Readability::NotReadable, "ghcr.io/acme/app").unwrap_err();
    assert_eq!(
        failure.to_string(),
        "ghcr.io/acme/app is not readable through this registry"
    );
}

#[test]
fn a_realm_change_proving_a_repository_is_shown_as_it_was_worded() {
    let detail = "proving a repository: the registry's challenge named the realm https://b, where only https://a is allowed";
    let failure = RegistryFailure::proving_repository(RegistryError::Refused {
        detail: detail.into(),
    });

    assert_eq!(failure, RegistryFailure::NotProven(detail.to_owned()));
    assert_eq!(failure.code(), "registry_not_proven");
}

#[test]
fn a_credential_missing_from_the_secret_partition_is_not_retryable() {
    let failure = RegistryFailure::CredentialUnreadable {
        host: "ghcr.io".into(),
    };
    assert_eq!(failure.status(), StatusCode::CONFLICT);
    assert_eq!(failure.code(), "registry_credential_unreadable");
}
