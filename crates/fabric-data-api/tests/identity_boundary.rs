//! The tenant comes from the issuer's registration, and nothing else can
//! change it.

mod support;

use axum::body::Body;
use fabric_data_api::API_PREFIX;
use fabric_identity::encode_unsigned_token;
use http::{Request, StatusCode};
use serde_json::json;
use support::{app, issuer_for, request};
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
    // Its token carries a *registered* issuer, so this exercises "registered
    // issuer, no claim" rather than "no issuer, no claim" — which is the case
    // the rule is actually about. The claim stays required after ADR 0019 §2.
    let (app, connector) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"sub": "user-123"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_token_cannot_name_a_tenant_its_issuer_does_not_own() {
    // The cross-tenant read this binding exists to close: a token minted by
    // `acme`'s realm, asking for `globex`. The tenant is `acme` or the request
    // is refused; it is never `globex`.
    let (app, connector) = app();

    let response = app
        .oneshot(request(
            "GET",
            "/customers",
            json!({"iss": issuer_for("acme"), "tenant_id": "globex"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0, "no query may reach a connector");
}

#[tokio::test]
async fn a_token_from_an_unregistered_issuer_is_refused_at_the_data_api() {
    // The edge refuses this independently. So does the runtime, which is what
    // makes registry drift between the two fail closed in both directions.
    let (app, connector) = app();

    let response = app
        .oneshot(request(
            "GET",
            "/customers",
            json!({"iss": "https://identity.test.invalid/realms/evil", "tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0, "no query may reach a connector");
}

#[tokio::test]
async fn a_token_with_no_issuer_is_refused_at_the_data_api() {
    // Refused, not treated as unregistered-but-harmless: an issuer check a
    // token can skip by omitting the claim is a control that does nothing.
    let (app, connector) = app();

    let serde_json::Value::Object(claims) = json!({"tenant_id": "acme"}) else {
        panic!("claims must be an object");
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("{API_PREFIX}/customers"))
                .header(
                    "authorization",
                    format!("Bearer {}", encode_unsigned_token(&claims)),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0, "no query may reach a connector");
}

#[tokio::test]
async fn a_claim_projection_header_changes_nothing_about_the_tenant() {
    // The edge strips these, and the gateway is forbidden to emit one. The
    // runtime's half is that it never reads one: the same token answers the
    // same way with and without it, and the connector sees the same tenant
    // predicate. A projected claim is inert, not a second identity source.
    let (app, connector) = app();

    let plain = app
        .clone()
        .oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))
        .await
        .unwrap();
    let expected = connector.last_query().1.filter;

    let mut projected = request("GET", "/customers", json!({"tenant_id": "globex"}));
    projected
        .headers_mut()
        .insert("x-jwt-claim-tenant-id", "acme".parse().unwrap());
    projected
        .headers_mut()
        .insert("x-forwarded-user", "somebody@acme.example".parse().unwrap());

    let response = app.oneshot(projected).await.unwrap();

    assert_eq!(plain.status(), StatusCode::OK);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(connector.query_count(), 2);
    assert_eq!(
        connector.last_query().1.filter,
        expected,
        "the projected header must not have changed the tenant"
    );
    assert!(
        expected.is_some(),
        "globex is discriminator-isolated, so there is a predicate to compare"
    );
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
