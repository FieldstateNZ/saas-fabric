//! A control plane that has never been connected to desired state.
//!
//! This is the state every deployment now starts in, and the whole point of
//! the change: the platform runs, answers, and says what is missing, so that
//! an operator can use the console to fix it. A control plane that refused to
//! start without a repository could not be used to connect one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use axum::body::Body;
use http::{header, Request, StatusCode};
use serde_json::Value;
use support::{as_operator, control_plane, json, send};

/// A GET as an authenticated operator.
fn get(path: &str) -> Request<Body> {
    as_operator("GET", path)
        .body(Body::empty())
        .expect("the request must build")
}

#[tokio::test]
async fn the_control_plane_serves_with_no_desired_state_repository() {
    let plane = control_plane();
    plane.binding.unbind();

    let response = send(&plane.router, get("/api/clients")).await;

    // Not a 500, and not an empty list. The platform is fine; it is not
    // connected to anything.
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "integration_not_configured");
}

#[tokio::test]
async fn not_configured_carries_no_retry_after_because_retrying_will_not_help() {
    let plane = control_plane();
    plane.binding.unbind();

    let response = send(&plane.router, get("/api/clients")).await;

    assert!(
        response.headers().get(header::RETRY_AFTER).is_none(),
        "a platform nobody has connected will not connect itself in five seconds"
    );
}

#[tokio::test]
async fn an_unconfigured_platform_reports_its_integration_as_not_configured() {
    let plane = control_plane();
    plane.binding.unbind();

    let response = send(&plane.router, get("/api/integrations/git")).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["status"], "not_configured");
    assert_eq!(body["connection"], Value::Null);
    assert_eq!(body["last_success_at"], Value::Null);
}

#[tokio::test]
async fn a_connected_platform_reports_the_repository_it_is_connected_to() {
    let plane = control_plane();

    let response = send(&plane.router, get("/api/integrations/git")).await;

    let body = json(response).await;
    assert_eq!(body["status"], "connected");
    assert!(
        body["connection"].is_string(),
        "a connected platform says what it is connected to"
    );
}

#[tokio::test]
async fn connecting_desired_state_takes_effect_without_a_restart() {
    let plane = control_plane();
    plane.binding.unbind();

    let refused = send(&plane.router, get("/api/clients")).await;
    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);

    plane.binding.bind(plane.repository.clone());

    let served = send(&plane.router, get("/api/clients")).await;
    assert_eq!(
        served.status(),
        StatusCode::OK,
        "an operator who connects a repository should not have to restart the platform"
    );
}

#[tokio::test]
async fn the_integration_status_is_not_public() {
    let plane = control_plane();

    // No operator header: whether this platform is connected, and to what, is
    // reconnaissance an unauthenticated caller should not get for free.
    let anonymous = Request::builder()
        .uri("/api/integrations/git")
        .body(Body::empty())
        .expect("the request must build");

    let response = send(&plane.router, anonymous).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_write_against_an_unconfigured_platform_is_refused_before_anything_is_read() {
    let plane = control_plane();
    plane.binding.unbind();

    let request = as_operator("PUT", "/api/clients/acme/identity")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
        .body(Body::from(
            serde_json::json!({
                "realm": "acme",
                "roles": ["Client Realm Administrator", "Client Realm User"],
                "clients": [{
                    "id": "web",
                    "type": "oidc",
                    "pkce": "s256",
                    "redirect": {
                        "strategy": "claimedHttps",
                        "uris": ["https://www.example.com/callback"],
                    },
                }],
            })
            .to_string(),
        ))
        .expect("the request must build");

    let response = send(&plane.router, request).await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json(response).await["error"]["code"],
        "integration_not_configured"
    );
}

#[tokio::test]
async fn the_platform_route_says_nothing_is_managed_rather_than_not_existing() {
    // The route takes no environment name. A deployment manages the one it was
    // deployed into, and a name in the URL would reach the platform repository
    // as a path segment -- which section 31.7 forbids and which is cheapest to
    // satisfy by giving a caller nowhere to say it.
    //
    // A deployment with no platform repository still mounts the route, for the
    // same reason the client routes stay mounted with no desired state: a
    // console can render "nothing is connected" and cannot render a 404 whose
    // meaning it would have to guess. The two are indistinguishable from the
    // browser otherwise -- a missing route and a missing integration look the
    // same, and only one of them is something an operator can fix.
    let plane = control_plane();

    let response = send(&plane.router, get("/api/platform")).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "platform_not_managed");
}

#[tokio::test]
async fn reading_the_platform_requires_an_operator() {
    // The composition of an environment is not public. An unauthenticated
    // caller learns nothing about which components exist or what they run.
    let plane = control_plane();

    let anonymous = Request::builder()
        .method("GET")
        .uri("/api/platform")
        .body(Body::empty())
        .expect("the request must build");

    let response = send(&plane.router, anonymous).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
