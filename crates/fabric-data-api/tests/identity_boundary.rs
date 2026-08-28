//! The tenant comes from the bearer token, and nothing else can change it.

mod support;

use axum::body::Body;
use fabric_data_api::API_PREFIX;
use fabric_identity::encode_unsigned_token;
use http::{Request, StatusCode};
use serde_json::json;
use support::{app, request};
use tower::ServiceExt as _;

#[tokio::test]
async fn a_tenant_header_is_rejected_outright() {
    // §11: there must be exactly one authoritative tenant context. Rejected
    // rather than ignored, so a caller who believes the header works is told.
    let (app, connector) = app();

    let serde_json::Value::Object(claims) = json!({"tenant_id": "acme"}) else {
        panic!("claims must be an object");
    };

    let request = Request::builder()
        .method("GET")
        .uri(format!("{API_PREFIX}/customers"))
        .header(
            "authorization",
            format!("Bearer {}", encode_unsigned_token(&claims)),
        )
        .header("x-tenant-id", "globex")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_request_with_no_token_is_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("{API_PREFIX}/customers"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0, "no query may reach a connector");
}

#[tokio::test]
async fn a_token_with_no_tenant_claim_is_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"sub": "user-123"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_token_with_an_invalid_tenant_claim_is_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "Acme Corp"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn an_expired_token_is_rejected_even_in_the_trusted_ingress_posture() {
    // Signatures are not verified here, but expiry still is: replaying a
    // captured token is cheap and refusing it costs one comparison.
    let (app, connector) = app();

    let response = app
        .oneshot(request(
            "GET",
            "/customers",
            json!({"tenant_id": "acme", "exp": 100}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0);
}
