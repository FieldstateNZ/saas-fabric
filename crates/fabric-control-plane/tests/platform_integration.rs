//! Two integrations, reachable at two sets of routes.
//!
//! What these pin is the boundary rather than the mechanism — the flow itself
//! is proven against fakes in the crate's own tests. Here the question is
//! whether the *API* keeps the two apart: whether a route exists for each,
//! whether one answers for the other, and whether an operator could name which
//! one they meant.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use axum::body::Body;
use http::{Request, StatusCode};
use serde_json::Value;
use support::{as_operator, control_plane, json, send};

/// A GET as an authenticated operator.
fn get(path: &str) -> Request<Body> {
    as_operator("GET", path)
        .body(Body::empty())
        .expect("the request must build")
}

#[tokio::test]
async fn a_deployment_that_manages_no_platform_says_so_rather_than_404() {
    // The route is mounted whether or not this deployment does platform
    // management, for the same reason the secrets routes are: a console can
    // tell an operator what is missing, and cannot tell them anything about a
    // route that does not exist.
    let plane = control_plane();

    let response = send(&plane.router, get("/api/integrations/platform")).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["managed"], false);
    assert_eq!(body["application"], Value::Null);
}

#[tokio::test]
async fn the_two_integrations_are_reported_by_two_routes() {
    // Not one route with a parameter. An operator's request never names which
    // integration it means; the path it was sent to is what decides.
    let plane = control_plane();

    let client = json(send(&plane.router, get("/api/integrations/git")).await).await;
    let platform = json(send(&plane.router, get("/api/integrations/platform")).await).await;

    // The client report answers about desired state; the platform report
    // answers about an application. Neither shape is the other's.
    assert!(client.get("status").is_some());
    assert!(
        platform.get("status").is_none(),
        "the platform integration does not report desired-state health; \
         /api/platform does, from the binding this connects"
    );
}

/// The status and the error code a route answered with.
///
/// The code is what separates "this route does not exist" from "this route
/// exists and this deployment has nothing behind it" — both of which are 404,
/// deliberately, and only one of which carries an error envelope.
async fn refusal(plane: &support::TestControlPlane, method: &str, path: &str) -> (StatusCode, String) {
    let request = as_operator(method, path)
        .body(Body::empty())
        .expect("the request must build");

    let response = send(&plane.router, request).await;
    let status = response.status();

    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("the body must be readable");

    let code = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|body: Value| {
            body.get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();

    (status, code)
}

#[tokio::test]
async fn there_is_no_route_that_takes_the_integration_as_a_name() {
    // The generalisation this design refused. A path segment naming an
    // integration is the shape section 15 forbids, and the one already closed
    // once in the environment parameter of `GET /api/platform`.
    let plane = control_plane();

    for path in [
        "/api/integrations/client-configuration",
        "/api/integrations/platform-management",
        "/api/integrations/anything",
    ] {
        let (status, code) = refusal(&plane, "GET", path).await;

        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(
            code.is_empty(),
            "{path} must be nothing at all, not a route answering that it has              no integration behind it"
        );
    }
}

#[tokio::test]
async fn every_platform_route_refuses_as_platform_management() {
    // This harness *has* a client integration route set and no platform one.
    // Each platform route refusing in platform management's own words is the
    // visible proof that none of them falls through to the other flow.
    let plane = control_plane();

    for (method, path) in [
        ("GET", "/api/integrations/platform/install"),
        ("GET", "/api/integrations/platform/repositories"),
        ("DELETE", "/api/integrations/platform"),
    ] {
        let (status, code) = refusal(&plane, method, path).await;

        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
        assert_eq!(code, "platform_not_managed", "{method} {path}");
    }
}

#[tokio::test]
async fn the_client_routes_still_refuse_as_client_configuration() {
    // The console and the deployment both already depend on these answers.
    // Adding a second flow was not allowed to move the first, and the code is
    // the part a console switches on.
    let plane = control_plane();

    for (method, path) in [
        ("GET", "/api/integrations/git/install"),
        ("GET", "/api/integrations/git/repositories"),
        ("DELETE", "/api/integrations/git"),
    ] {
        let (status, code) = refusal(&plane, method, path).await;

        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
        assert_eq!(code, "integration_not_managed", "{method} {path}");
    }
}
