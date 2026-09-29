//! A refused selection's statuses, codes and body.

use axum::response::IntoResponse as _;
use fabric_client_model::ClientId;
use fabric_platform_management::InvalidReason;
use http::StatusCode;

use crate::{ControlPlaneError, RegistryFailure, SelectionRefusal, Unusable};

fn id(value: &str) -> ClientId {
    ClientId::try_new(value).unwrap()
}

fn unusable(answer: Unusable) -> SelectionRefusal {
    SelectionRefusal::Unusable {
        repository: "ghcr.io/acme/reports".to_owned(),
        version: "1.4.0".to_owned(),
        answer,
    }
}

/// Every refusal, with the status and code it answers.
fn every() -> Vec<(SelectionRefusal, StatusCode, &'static str)> {
    vec![
        (
            SelectionRefusal::RepositoryNotRegistered {
                repository: "ghcr.io/acme/reports".to_owned(),
            },
            StatusCode::UNPROCESSABLE_ENTITY,
            "repository_not_registered",
        ),
        (
            SelectionRefusal::VersionNotFound {
                repository: "ghcr.io/acme/reports".to_owned(),
                version: "9.9.9".to_owned(),
            },
            StatusCode::UNPROCESSABLE_ENTITY,
            "component_version_not_found",
        ),
        (
            unusable(Unusable::Undescribed),
            StatusCode::UNPROCESSABLE_ENTITY,
            "component_version_unusable",
        ),
        (
            SelectionRefusal::AlreadySelected {
                application: id("analytics"),
                component: id("reports"),
                descriptor_digest: format!("sha256:{}", "a".repeat(64)),
            },
            StatusCode::CONFLICT,
            "component_version_already_selected",
        ),
        (
            SelectionRefusal::CapabilityNotSelectable {
                application: id("analytics"),
                component: id("identity"),
            },
            StatusCode::UNPROCESSABLE_ENTITY,
            "capability_not_selectable",
        ),
    ]
}

#[test]
fn every_refusal_has_its_own_code_and_none_is_a_registry_failure_or_retryable() {
    let registry_codes = [
        RegistryFailure::Refused(String::new()).code(),
        RegistryFailure::Unavailable(String::new()).code(),
        RegistryFailure::NotFound { host: String::new() }.code(),
    ];
    let mut codes = Vec::new();
    for (refusal, status, code) in every() {
        let error = ControlPlaneError::from(refusal);
        assert_eq!(error.status(), status, "{code}");
        assert_eq!(error.code(), code);
        assert!(!registry_codes.contains(&code), "{code}");
        let response = error.into_response();
        assert!(
            response.headers().get(http::header::RETRY_AFTER).is_none(),
            "{code}"
        );
        codes.push(code);
    }
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), every().len(), "two refusals share a code");
}

#[test]
fn only_an_unusable_version_carries_the_answer_and_only_invalid_a_reason() {
    assert_eq!(
        unusable(Unusable::Undescribed).answer(),
        Some(("undescribed", None))
    );
    assert_eq!(
        unusable(Unusable::Incoherent).answer(),
        Some(("incoherent", None))
    );
    assert_eq!(
        unusable(Unusable::Invalid(InvalidReason::Several)).answer(),
        Some(("invalid", Some("several")))
    );
    for (refusal, _, code) in every() {
        let carries = matches!(refusal, SelectionRefusal::Unusable { .. });
        assert_eq!(refusal.answer().is_some(), carries, "{code}");
    }
}

#[tokio::test]
async fn the_body_names_the_answer_and_reason_beside_the_code() {
    let error = ControlPlaneError::from(unusable(Unusable::Invalid(InvalidReason::NotRegistered {
        repository: "ghcr.io/acme/reports-web".to_owned(),
    })));

    let response = error.into_response();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(body["error"]["code"], "component_version_unusable");
    assert_eq!(body["error"]["answer"], "invalid");
    assert_eq!(body["error"]["reason"], "notRegistered");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("ghcr.io/acme/reports 1.4.0"), "{message}");
    assert!(
        message.contains("ghcr.io/acme/reports-web is not registered"),
        "{message}"
    );
}
