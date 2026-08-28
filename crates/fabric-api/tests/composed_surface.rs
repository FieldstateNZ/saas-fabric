//! The paths the composition root actually serves.
//!
//! Every other test in this workspace exercises a router built by one crate.
//! This one exercises the router the process listens on, because the mistakes
//! it guards against can only happen where the pieces are joined.
//!
//! `fabric-data-api` owns its whole external path — `/v1/data/...`, exported
//! as `API_PREFIX` — and the host therefore `merge`s it rather than nesting a
//! prefix in front. Nesting instead would compile, pass every test in
//! `fabric-data-api`, and quietly serve `/data/v1/customers`: every client
//! broken, no test red. The assertions below are the only place that
//! distinction is visible.
//!
//! # This file used to lie
//!
//! It said the router was "assembled exactly as `startup::application::build`
//! does" and then hand-assembled a different one: no `TimeoutLayer`, no probe
//! routes, no `TraceLayer`. A test that rebuilds the thing it is checking is
//! checking itself, and the missing probe routes are how a `/ready` with no
//! bound on its own I/O got through review. The join is now
//! [`fabric_api::startup::compose`], called by production and by this file,
//! so the claim is true by construction rather than by care.
//!
//! No connector negotiation happens here, so the `/v1/data` assertions are
//! routing assertions only. A request reaching a real handler is rejected for
//! want of an identity — which is the point: **401 proves the route exists**,
//! and 404 proves it does not.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::Router;
use examples_support::probe_connector::{Health, ProbeConnector};
use examples_support::stub_connector::StubConnector;
use examples_support::{catalog, config, data_sources, tenants};
use fabric_api::health::{health_routes, HealthState};
use fabric_api::startup::compose;
use fabric_connector::{ConnectorRegistry, DataConnector};
use fabric_core::SystemClock;
use fabric_data_api::{build_data_api, API_PREFIX};
use fabric_identity::{build_identity, TrustedIngressReader};
use fabric_tenant_runtime::{DataSourceRegistry, RuntimeResolver, TenantRegistry};
use http::{Request, StatusCode};
use tower::ServiceExt;

/// The host's router, assembled by the same function the composition root
/// uses.
///
/// The connector is a stub. Nothing in these tests executes an operation, and
/// requiring a real one would mean this file could only run against a live
/// NDC process.
fn router() -> Router {
    let stub = StubConnector::new(
        config()
            .connectors
            .first()
            .expect("the example configures a connector")
            .id
            .as_str(),
    );

    router_with(
        Duration::from_secs(config().request_timeout_seconds),
        Arc::new(stub),
    )
}

/// The same assembly, with the Data API's timeout budget and the registered
/// connector chosen by the caller.
fn router_with(request_timeout: Duration, connector: Arc<dyn DataConnector>) -> Router {
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

    let runtime = Arc::new(RuntimeResolver::new(tenant_registry, data_source_registry));

    // Registration requires at least one connector; nothing here reaches it
    // for execution, and it refuses every operation if anything tries.
    let connectors = ConnectorRegistry::new().with(connector);

    let data = build_data_api(
        &config.data_api,
        catalog(),
        config.permissions.clone(),
        Arc::clone(&runtime),
        connectors.clone(),
        Arc::clone(&identity),
    )
    .expect("the Data API must build from the example configuration");

    let health = health_routes(HealthState {
        runtime,
        connectors,
        identity,
        administrator_role: config.permissions.administrator_role.clone(),
    });

    compose(data, health, request_timeout)
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

#[tokio::test]
async fn the_liveness_probe_is_mounted_alongside_the_data_api() {
    assert_eq!(status_of("/health").await, StatusCode::OK);
}

#[tokio::test]
async fn the_readiness_probe_is_mounted_alongside_the_data_api() {
    // The route this file did not know existed. The example fixtures hold
    // tenants and DataSources and the stub connector refuses its health
    // check, so the verdict is 503 — what matters here is that it is not 404.
    let status = status_of("/ready").await;

    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "the composition root mounts /ready; this assembly must too"
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn the_probes_sit_outside_the_data_api_timeout_scope() {
    // A one-millisecond Data API budget, and a probe that takes far longer
    // than that. If the timeout were applied to the whole router instead of
    // to `data`, this would be a 504 — which a kubelet records as a failed
    // probe and answers by pulling the replica, the precise outcome the
    // readiness policy exists to prevent.
    let router = router_with(
        Duration::from_millis(1),
        Arc::new(ProbeConnector::new(
            "postgres-au-east",
            Health::Slow(Duration::from_millis(80)),
        )),
    );

    let started = Instant::now();
    let status = router
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status();

    assert!(
        started.elapsed() > Duration::from_millis(1),
        "the probe must outlast the budget for this to be testing anything"
    );
    assert_ne!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(status, StatusCode::OK);
}
