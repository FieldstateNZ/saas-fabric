//! What must never leave the control plane.
//!
//! These are boundary assertions rather than behaviour tests: each one pins a
//! property that, if it broke, would break silently and be found by somebody
//! reading a browser's network tab rather than by a failing build.
//!
//! The structural half of the same boundary — that this crate cannot even
//! *name* a Keycloak type, and that no runtime crate can reach the Keycloak
//! adapter — is enforced by `scripts/check_architecture.py`, because those are
//! statements about what the workspace may contain and no unit test can fail
//! when they are violated.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole. Clippy's `allow-unwrap-in-tests` only covers
// `#[test]` functions, so an integration test file states it once here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use axum::body::Body;
use fabric_client_model::{ClientId, ClientRevision};
use fabric_reconciliation::{ProviderError, ReconciliationOutcome, ReconciliationStatus};
use support::{as_operator, control_plane, json, send};

/// Words that must never appear in anything the browser receives.
///
/// Two groups. The first is credential vocabulary of any kind: the control
/// plane holds a Keycloak administrative credential and a Git token, and
/// neither may reach a response, in any field, under any name. The second is
/// repository internals — a path or a branch — which specification §8 keeps
/// out of the API's vocabulary.
const FORBIDDEN: [&str; 10] = [
    "client_secret",
    "clientSecret",
    "password",
    "access_token",
    "Authorization",
    "Bearer ",
    "admin-cli",
    "client.yaml",
    "clients/acme",
    "refs/heads",
];

/// Fails if a response body contains anything from [`FORBIDDEN`].
fn assert_nothing_forbidden(body: &serde_json::Value) {
    let rendered = body.to_string();

    for forbidden in FORBIDDEN {
        assert!(
            !rendered.contains(forbidden),
            "a response carried {forbidden}: {rendered}"
        );
    }
}

#[tokio::test]
async fn no_endpoint_returns_a_credential_or_a_repository_path() {
    let plane = control_plane();

    for path in ["/api/clients", "/api/clients/acme", "/api/clients/acme/identity"] {
        let response = send(
            &plane.router,
            as_operator("GET", path).body(Body::empty()).unwrap(),
        )
        .await;

        assert_nothing_forbidden(&json(response).await);
    }
}

#[tokio::test]
async fn a_provider_failure_reaches_the_operator_without_the_provider_speaking() {
    // The detail an operator is shown is written by the adapter, which is
    // required to keep upstream response bodies out of it. This pins that the
    // path exists and that nothing else is added on the way through.
    let plane = control_plane();

    plane.statuses.record(
        &ClientId::try_new("acme").unwrap(),
        plane.revision.clone(),
        &ReconciliationOutcome::failed(&ProviderError::Unavailable {
            detail: "connection refused".to_owned(),
        }),
        1_700_000_000,
    );

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients/acme/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    let body = json(response).await;

    assert_eq!(body["reconciliation"]["status"], "failed");
    assert!(body["reconciliation"]["detail"]
        .as_str()
        .is_some_and(|detail| detail.contains("connection refused")));
    assert_nothing_forbidden(&body);
}

#[tokio::test]
async fn reconciliation_status_is_visible_through_the_api() {
    // Acceptance criterion: an operator can see whether desired state has
    // taken effect, without asking Keycloak.
    let plane = control_plane();
    let client = ClientId::try_new("acme").unwrap();

    // The sequence is the state machine, not two independent cases: a client
    // that was converged and then needed correcting at the *same* revision has
    // been changed by something outside SaaS Fabric, and the API says so.
    for (recorded, expected) in [
        (ReconciliationOutcome::applied(4), "applied"),
        (ReconciliationOutcome::converged(), "applied"),
        (ReconciliationOutcome::applied(2), "drifted"),
    ] {
        plane
            .statuses
            .record(&client, plane.revision.clone(), &recorded, 1_700_000_000);

        let response = send(
            &plane.router,
            as_operator("GET", "/api/clients/acme/identity")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(json(response).await["reconciliation"]["status"], expected);
    }
}

#[tokio::test]
async fn a_status_recorded_against_another_revision_is_shown_as_pending() {
    let plane = control_plane();

    plane.statuses.record(
        &ClientId::try_new("acme").unwrap(),
        ClientRevision::try_new("rev-999").unwrap(),
        &ReconciliationOutcome::converged(),
        1_700_000_000,
    );

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients/acme/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(json(response).await["reconciliation"]["status"], "pending");
}

#[tokio::test]
async fn the_api_never_offers_a_way_to_reach_the_identity_provider() {
    // There is no route that proxies to Keycloak, and no route that would let
    // an operator hand the platform a credential to use.
    let plane = control_plane();

    for path in [
        "/api/keycloak",
        "/api/clients/acme/keycloak",
        "/api/clients/acme/realm",
        "/api/clients/acme/identity/credentials",
    ] {
        let response = send(
            &plane.router,
            as_operator("GET", path).body(Body::empty()).unwrap(),
        )
        .await;

        assert_eq!(
            response.status(),
            http::StatusCode::NOT_FOUND,
            "{path} is served, and it should not be"
        );
    }
}

#[tokio::test]
async fn the_reconciliation_status_vocabulary_is_the_documented_one() {
    // The UI branches on these four strings. A rename would change what an
    // operator is shown without changing anything that fails to compile.
    let names: Vec<&str> = [
        ReconciliationStatus::Pending,
        ReconciliationStatus::Applied,
        ReconciliationStatus::Failed,
        ReconciliationStatus::Drifted,
    ]
    .iter()
    .map(|status| status.as_str())
    .collect();

    assert_eq!(names, ["pending", "applied", "failed", "drifted"]);
}
