//! The paths the composition root actually serves.
//!
//! Every other test in this workspace exercises a router built by one crate.
//! This one exercises the router the process listens on, because the mistake
//! it guards against can only happen where the two are joined.
//!
//! `fabric-data-api` owns its whole external path — `/v1/data/...`, exported
//! as `API_PREFIX` — and the host therefore `merge`s it rather than nesting a
//! prefix in front. Nesting instead would compile, pass every test in
//! `fabric-data-api`, and quietly serve `/data/v1/customers`: every client
//! broken, no test red. The assertions below are the only place that
//! distinction is visible.
//!
//! No connector negotiation happens here, so these are routing assertions
//! only. A request reaching a real handler is rejected for want of an
//! identity — which is the point: **401 proves the route exists**, and 404
//! proves it does not.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use std::sync::Arc;

use axum::body::Body;
use axum::Router;
use examples_support::stub_connector::StubConnector;
use examples_support::{catalog, config, data_sources, tenants};
use fabric_connector::ConnectorRegistry;
use fabric_core::SystemClock;
use fabric_data_api::{build_data_api, API_PREFIX};
use fabric_identity::{build_identity, TrustedIngressReader};
use fabric_tenant_runtime::{DataSourceRegistry, RuntimeResolver, TenantRegistry};
use http::{Request, StatusCode};
use tower::ServiceExt;

/// The host's router, assembled exactly as `startup::application::build` does.
///
/// The connector is a stub. Nothing in these tests reaches one, and
/// requiring a real one would mean this file could only run against a live
/// NDC process.
fn router() -> Router {
    let config = config();

    let tenant_registry = Arc::new(TenantRegistry::new());
    assert!(
        tenant_registry.apply_all(tenants()).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    let data_source_registry = Arc::new(DataSourceRegistry::new());
    assert!(
        data_source_registry.apply_all(data_sources()).is_ok(),
        "the fixture must install; a first load this test cannot use is a broken fixture"
    );

    let identity = build_identity(
        config.identity.clone(),
        Arc::new(TrustedIngressReader::new(SystemClock::shared())),
    )
    .expect("the example identity configuration must build");

    let data = build_data_api(
        &config.data_api,
        catalog(),
        config.permissions.clone(),
        Arc::new(RuntimeResolver::new(tenant_registry, data_source_registry)),
        // Registration requires at least one connector; nothing here
        // reaches it, and it refuses every operation if anything tries.
        ConnectorRegistry::new().with(Arc::new(StubConnector::new(
            config
                .connectors
                .first()
                .expect("the example configures a connector")
                .id
                .as_str(),
        ))),
        identity,
    )
    .expect("the Data API must build from the example configuration");

    Router::new().merge(data)
}

async fn status_of(path: &str) -> StatusCode {
    router()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[test]
fn the_data_api_prefix_carries_the_version_in_front() {
    // If this ever becomes `/data/v1`, the host is nesting a prefix in front
    // of a router that already has one.
    assert_eq!(API_PREFIX, "/v1/data");
}

#[tokio::test]
async fn the_versioned_data_path_is_served() {
    // Not 404: the route is mounted. It is 401 because the request carries
    // no credentials at all — distinct from the 403 an authenticated caller
    // with nothing here would get. Asserting the exact code, not just
    // "not 404", is what stops a future change from turning this into a 404
    // that still reads as a pass.
    let status = status_of("/v1/data/customers").await;

    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "the versioned data path must be mounted"
    );
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_keyed_data_path_is_served() {
    let status = status_of("/v1/data/customers/some-key").await;

    assert_ne!(status, StatusCode::NOT_FOUND);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_unversioned_path_is_not_served() {
    // The failure this whole file exists for. `nest("/data", ..)` over a
    // router that already carries `/v1/data` would leave this 404 *and* make
    // the real path `/data/v1/customers`, so this assertion alone is not
    // enough — it is the pair with the two above that pins the shape.
    assert_eq!(status_of("/data/customers").await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_version_never_appears_after_the_data_segment() {
    // The exact path a `nest` would have produced.
    assert_eq!(status_of("/data/v1/customers").await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_prefix_is_not_a_catch_all() {
    // Everything above would also pass if the prefix answered 401 for any
    // path and method under it. A verb the resource routes do not declare
    // gets 405, which can only come from a router that registered specific
    // methods on a specific path -- so this is what distinguishes a real
    // mount from a blanket fallback.
    let response = router()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/data/customers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
