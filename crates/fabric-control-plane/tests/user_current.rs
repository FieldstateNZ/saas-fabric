//! `GET /api/user/current` through the router `build_control_plane` returns.
//!
//! The authenticator here is the fixture one, which verifies nothing; what
//! these tests pin is the wiring around it — the route is mounted behind the
//! operator extractor, the reported role is the one the composition supplied,
//! and a bearer the profile reader cannot read is never a `200`. Real signed
//! tokens through the OIDC authenticator are `get_current_user_tests.rs`'s.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use axum::body::Body;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use fabric_control_plane::testing::AcceptingOperator;
use fabric_control_plane::SuppliedOperators;
use http::StatusCode;
use serde_json::{json, Value};
use support::{
    as_operator, control_plane, control_plane_with_operators, json, send, OPERATOR, PRESENTED_ROLE,
};

/// A bearer shaped like a token, carrying `payload` as its claims.
fn bearer(payload: &[u8]) -> String {
    format!(
        "{}.{}.{}",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#),
        URL_SAFE_NO_PAD.encode(payload),
        URL_SAFE_NO_PAD.encode(b"fixture-signature")
    )
}

fn claims(claims: &Value) -> String {
    bearer(claims.to_string().as_bytes())
}

fn presenting(bearer: &str, role: &str) -> SuppliedOperators {
    SuppliedOperators {
        authenticator: AcceptingOperator::accepting_with_bearer(OPERATOR, bearer),
        presented_role: role.to_owned(),
    }
}

async fn ask(operators: SuppliedOperators) -> (StatusCode, String) {
    let plane = control_plane_with_operators(operators);
    let response = send(
        &plane.router,
        as_operator("GET", "/api/user/current")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn an_anonymous_request_is_unauthenticated() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        http::Request::builder()
            .uri("/api/user/current")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json(response).await["error"]["code"], "unauthenticated");
}

#[tokio::test]
async fn an_operator_reads_the_profile_with_the_supplied_role() {
    let token = claims(&json!({
        "sub": "a7c1e0f2",
        "preferred_username": "brett",
        "email": "brett@example.test",
        "name": "Brett Example",
        "realm_access": { "roles": [PRESENTED_ROLE, "offline_access"] },
    }));

    let (status, body) = ask(presenting(&token, PRESENTED_ROLE)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({
            "subject": "a7c1e0f2",
            "userName": "brett",
            "displayName": "Brett Example",
            "email": "brett@example.test",
            "roles": [PRESENTED_ROLE],
        })
    );
}

#[tokio::test]
async fn the_role_is_the_composition_s_and_never_the_token_s() {
    let token = claims(&json!({
        "sub": "a7c1e0f2",
        "realm_access": { "roles": ["fabric-operator", "realm-admin"] },
    }));

    let (_, body) = ask(presenting(&token, "platform-admin")).await;

    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["roles"],
        json!(["platform-admin"])
    );
}

#[tokio::test]
async fn an_accepted_bearer_with_no_readable_profile_is_never_a_200() {
    let cases = [
        ("not a token", "a-fixture-bearer".to_owned()),
        ("two segments", "aGVhZGVy.cGF5bG9hZA".to_owned()),
        (
            "four segments",
            format!("{}.extra", claims(&json!({ "sub": "x" }))),
        ),
        ("payload not base64", "aGVhZGVy.!!!.c2ln".to_owned()),
        ("payload not JSON", bearer(b"not json")),
        ("payload not an object", bearer(br#"["sub"]"#)),
        ("no sub", claims(&json!({ "preferred_username": "brett" }))),
        (
            "blank sub",
            claims(&json!({ "sub": "   ", "preferred_username": "brett" })),
        ),
        (
            "sub not a string",
            claims(&json!({ "sub": 42, "preferred_username": "brett" })),
        ),
    ];

    for (case, token) in cases {
        let (status, body) = ask(presenting(&token, PRESENTED_ROLE)).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{case}");
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["error"]["code"], "profile_unreadable", "{case}");
        assert!(parsed.get("subject").is_none(), "{case}");
        assert!(!body.contains(&token), "{case}: the bearer reached the body");
        assert!(!body.contains("brett"), "{case}: a claim reached the body");
    }
}

#[tokio::test]
async fn the_operator_route_still_answers_the_subject_alone() {
    let token = claims(&json!({ "sub": "a7c1e0f2", "preferred_username": "brett" }));
    let plane = control_plane_with_operators(presenting(&token, PRESENTED_ROLE));

    let response = send(
        &plane.router,
        as_operator("GET", "/api/operator").body(Body::empty()).unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json(response).await, json!({ "subject": OPERATOR }));
}
