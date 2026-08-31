//! What the secrets endpoints put on the wire, and what they must never.
//!
//! These are the invariants the whole slice exists to hold. Each one is cheap
//! to break by accident — a header nobody set, a value included in a listing
//! "for convenience", a conflict flattened into a 500 — and none of them fails
//! loudly when broken.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use support::{control_plane, OPERATOR, OPERATOR_HEADER, SECRET_VALUE};
use tower::ServiceExt as _;

/// Sends a request as an authenticated operator, returning status, headers and
/// body.
async fn as_operator(
    method: &str,
    path: &str,
    body: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let harness = control_plane();

    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(OPERATOR_HEADER, OPERATOR);

    if body.is_some() {
        request = request.header("content-type", "application/json");
    }

    let response = harness
        .router
        .oneshot(
            request
                .body(body.map_or_else(Body::empty, |text| Body::from(text.to_owned())))
                .expect("a request"),
        )
        .await
        .expect("a response");

    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");

    (status, headers, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn revealing_a_secret_is_never_stored_by_anything_in_between() {
    let (status, headers, body) = as_operator(
        "POST",
        "/api/clients/acme/secrets/reveal",
        Some(r#"{"path":"database/primary"}"#),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(SECRET_VALUE),
        "reveal is the one response that carries it"
    );

    // Set explicitly rather than left to a default. Without it a proxy, a
    // browser cache or a disk cache is free to keep the one response in this
    // API that carries a secret — a copy nobody knows about and nobody can
    // revoke.
    let cache = headers
        .get(axum::http::header::CACHE_CONTROL)
        .expect("reveal must say how it may be stored")
        .to_str()
        .expect("a readable header");

    assert!(cache.contains("no-store"), "reveal must be no-store, got {cache}");
}

#[tokio::test]
async fn no_other_secret_response_carries_a_value() {
    // The operations a console performs constantly. If a value ever appears
    // here, it appears on every page load and in every intermediary's logs.
    let (_, _, listing) = as_operator("GET", "/api/clients/acme/secrets", None).await;
    assert!(!listing.contains(SECRET_VALUE), "a listing must not carry values");

    let (_, _, metadata) = as_operator("GET", "/api/clients/acme/secrets/entry/database/primary", None).await;
    assert!(!metadata.contains(SECRET_VALUE), "metadata must not carry values");
    assert!(
        metadata.contains("\"version\""),
        "metadata says what it does carry"
    );

    let (_, _, written) = as_operator(
        "PUT",
        "/api/clients/acme/secrets/entry/database/primary",
        Some(r#"{"values":{"password":"replacement"},"expectedVersion":7}"#),
    )
    .await;
    assert!(
        !written.contains("replacement"),
        "a write must not echo back what was written"
    );
}

#[tokio::test]
async fn a_stale_write_is_a_conflict_an_operator_can_act_on() {
    let (status, _, body) = as_operator(
        "PUT",
        "/api/clients/acme/secrets/entry/database/primary",
        // Version 1, when the store is at 7: somebody else wrote first.
        Some(r#"{"values":{"password":"x"},"expectedVersion":1}"#),
    )
    .await;

    // Not a 500. A working system behaving correctly must not send an operator
    // to an incident channel.
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body.contains("secret_stale_version"),
        "the console branches on the code, not the status: {body}"
    );
}

#[tokio::test]
async fn a_path_that_climbs_out_of_the_client_is_refused() {
    // The boundary is enforced by resolving a namespace from trusted state and
    // prefixing it. A path that can climb out makes that decorative.
    for attempt in [
        "/api/clients/acme/secrets/entry/../../other",
        "/api/clients/acme/secrets/entry/database/../../../other",
    ] {
        let (status, _, _) = as_operator("GET", attempt, None).await;

        assert!(status != StatusCode::OK, "{attempt} must not resolve to a secret");
    }

    let (status, _, _) = as_operator(
        "POST",
        "/api/clients/acme/secrets/reveal",
        Some(r#"{"path":"../../other"}"#),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a traversal is the caller's error"
    );
}

#[tokio::test]
async fn a_secret_route_cannot_be_reached_without_an_operator() {
    let harness = control_plane();

    let response = harness
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/clients/acme/secrets/reveal")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"path":"database/primary"}"#))
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
