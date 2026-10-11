//! SaaS Fabric's own rollback floor (D01-21), through the real router with
//! the floor the shipping binary is built with.
//!
//! `fabric-platform-management` proves the rule against fakes. These pin
//! what an operator meets: the listing never offers a version below the
//! floor, and naming one anyway is its own refusal, decided before the
//! platform repository is reached.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;

use axum::body::Body;
use fabric_component::MINIMUM_ROLLABLE_VERSION;
use fabric_platform_management::{Registry, RollbackFloors};
use http::{Request, StatusCode};
use serde_json::{json, Value};
use support::described_registry::{described_at, DescribedRegistry};
use support::platform_fixture::platform_binding_with_floors;
use support::{as_operator, control_plane_with_platform, json, send, TestControlPlane};

const COMMIT: &str = "5320432aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// The last release before the formats the floor protects.
const BELOW: &str = "0.3.0-preview.15";

/// One release above the floor.
const ABOVE: &str = "0.3.0-preview.17";

/// What the environment runs.
const DESIRED: &str = "0.3.0-preview.18";

/// A control plane managing SaaS Fabric at [`DESIRED`], with complete
/// releases at [`BELOW`], the floor and [`ABOVE`].
async fn plane() -> TestControlPlane {
    let registry = DescribedRegistry::default();
    for version in [BELOW, MINIMUM_ROLLABLE_VERSION, ABOVE] {
        registry.publish(version, COMMIT);
    }
    let floors = RollbackFloors::for_this_release().unwrap();
    let (platform, _fake) = platform_binding_with_floors(
        Arc::new(registry) as Arc<dyn Registry>,
        vec![described_at(DESIRED)],
        floors,
    )
    .await;
    control_plane_with_platform(platform)
}

fn roll_back_to(version: &str) -> Request<Body> {
    as_operator("POST", "/api/platform/components/saas-fabric/rollback")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({ "version": version }).to_string()))
        .expect("the request must build")
}

#[tokio::test]
async fn the_listing_stops_at_the_floor_and_says_there_is_nothing_older() {
    let plane = plane().await;

    let response = send(
        &plane.router,
        as_operator("GET", "/api/platform/components/saas-fabric/versions")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = json(response).await;

    assert_eq!(
        body,
        json!({
            "versions": [
                { "version": ABOVE, "source_revision": COMMIT },
                { "version": MINIMUM_ROLLABLE_VERSION, "source_revision": COMMIT },
            ],
            "more": false,
        })
    );
}

#[tokio::test]
async fn a_rollback_below_the_floor_is_refused_as_its_own_code() {
    let plane = plane().await;

    let response = send(&plane.router, roll_back_to(BELOW)).await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "below_rollback_floor");
}

#[tokio::test]
async fn a_rollback_to_the_floor_itself_passes_the_floor() {
    // The fixture's platform repository refuses every write, so getting as
    // far as that refusal is what proves the floor let the version through.
    let plane = plane().await;

    let response = send(&plane.router, roll_back_to(MINIMUM_ROLLABLE_VERSION)).await;

    let body = json(response).await;
    assert_ne!(body["error"]["code"], "below_rollback_floor", "{body}");
    assert_ne!(body["error"]["code"], "version_not_rollable", "{body}");
}
