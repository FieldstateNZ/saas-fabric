//! The control-plane API's contract, over HTTP.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole. Clippy's `allow-unwrap-in-tests` only covers
// `#[test]` functions, so an integration test file states it once here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use axum::body::Body;
use fabric_client_model::ClientId;
use fabric_control_plane::ClientRepository as _;
use http::{header, StatusCode};
use support::{as_operator, control_plane, entity_tag, json, send};

/// The identity an operator would submit after adding a role.
fn identity_with_extra_role() -> Body {
    Body::from(
        serde_json::json!({
            "realm": "acme",
            "roles": ["Client Realm Administrator", "Client Realm User", "Invoicing Approver"],
            "clients": [{
                "id": "web",
                "type": "oidc",
                "redirectUris": ["https://www.example.com/callback"],
            }],
        })
        .to_string(),
    )
}

#[tokio::test]
async fn listing_clients_returns_the_desired_state_source() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients").body(Body::empty()).unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = json(response).await;
    assert_eq!(body["clients"][0]["id"], "acme");
    assert_eq!(body["clients"][0]["displayName"], "Acme");
    assert_eq!(body["clients"][0]["hosts"][0], "www.example.com");
}

#[tokio::test]
async fn a_client_detail_names_its_realm_but_not_its_roles() {
    // Roles belong to the identity view. Two copies is one that can be stale.
    let plane = control_plane();

    let body = json(
        send(
            &plane.router,
            as_operator("GET", "/api/clients/acme")
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;

    assert_eq!(body["realm"], "acme");
    assert!(body.get("roles").is_none());
}

#[tokio::test]
async fn identity_is_returned_with_its_revision_and_reconciliation_state() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients/acme/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    let tag = entity_tag(&response);
    let body = json(response).await;

    assert_eq!(body["realm"], "acme");
    assert_eq!(body["roles"][0], "Client Realm Administrator");
    assert_eq!(body["clients"][0]["id"], "web");
    assert_eq!(body["revision"], tag, "the entity tag and the body must agree");

    // Nothing has reconciled it yet, and the API says so rather than implying
    // the document is reality.
    assert_eq!(body["reconciliation"]["status"], "pending");
}

#[tokio::test]
async fn a_write_at_the_current_revision_is_accepted_and_reports_pending() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = json(response).await;
    assert_eq!(body["roles"][2], "Invoicing Approver");
    assert_ne!(body["revision"], plane.revision.as_str());

    // The write succeeded; Keycloak has provably not been touched.
    assert_eq!(body["reconciliation"]["status"], "pending");
}

#[tokio::test]
async fn a_write_at_a_stale_revision_is_a_conflict() {
    let plane = control_plane();
    let stale = format!("\"{}\"", plane.revision);

    let first = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, &stale)
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let second = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, &stale)
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;

    assert_eq!(second.status(), StatusCode::CONFLICT);
    assert_eq!(json(second).await["error"]["code"], "revision_conflict");
}

#[tokio::test]
async fn a_write_without_if_match_is_refused_rather_than_applied() {
    // Last-writer-wins is what this status exists to prevent.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(json(response).await["error"]["code"], "revision_required");
}

#[tokio::test]
async fn changing_the_realm_is_refused() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "realm": "somewhere-else",
                    "roles": ["Client Realm Administrator", "Client Realm User"],
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["error"]["code"], "realm_immutable");
}

#[tokio::test]
async fn removing_a_required_role_is_refused() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "realm": "acme",
                    "roles": ["Client Realm User"],
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn an_unknown_client_is_a_not_found() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients/nobody/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(json(response).await["error"]["code"], "unknown_client");
}

#[tokio::test]
async fn an_edit_preserves_sections_the_control_plane_does_not_model() {
    let plane = control_plane();

    send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;

    let stored = plane
        .repository
        .get(&ClientId::try_new("acme").unwrap())
        .await
        .expect("the client must still be there");

    let document = stored.document.render().expect("the stored document must render");
    assert!(document.contains("invoicing: true"));
    assert!(document.contains("Invoicing Approver"));
}

#[tokio::test]
async fn an_unknown_field_in_the_body_is_refused_rather_than_ignored() {
    // Accepting it would report success for a change that did not happen.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "realm": "acme",
                    "roles": ["Client Realm Administrator", "Client Realm User"],
                    "keycloakRealmSettings": {"bruteForceProtected": true},
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn a_malformed_body_is_refused_in_this_api_s_error_shape() {
    // Axum's own rejection would answer in a different shape, and the console
    // branches on `error.code`.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{not json"))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn a_client_id_that_could_not_exist_is_refused_in_this_api_s_error_shape() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients/NOT_A_CLIENT/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = json(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");
    // The value is caller-controlled and reaches here from a URL; reflecting it
    // would turn the error body into a mirror.
    assert!(!body["error"]["message"].to_string().contains("NOT_A_CLIENT"));
}

#[tokio::test]
async fn an_unauthenticated_request_reaches_no_handler() {
    let plane = control_plane();

    for (method, path) in [
        ("GET", "/api/clients"),
        ("GET", "/api/clients/acme"),
        ("GET", "/api/clients/acme/identity"),
        ("PUT", "/api/clients/acme/identity"),
    ] {
        let response = send(
            &plane.router,
            http::Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} answered without an operator"
        );
    }
}

#[tokio::test]
async fn a_request_carrying_no_operator_identity_is_refused() {
    // This used to assert that a name outside the allowlist was refused, which
    // was a property of the trusted-header posture. That posture is gone: who
    // counts as an operator is now a realm role, checked against a verified
    // token by `OidcOperators`' own tests.
    //
    // What still belongs here is the property this *router* has to have —
    // every path under `/api/clients` refuses a request that established no
    // operator at all, which is the extractor doing its job.
    let plane = control_plane();

    let response = send(
        &plane.router,
        http::Request::builder()
            .method("GET")
            .uri("/api/clients")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json(response).await["error"]["code"], "unauthenticated");
}
