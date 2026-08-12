//! Item 37: a dropped handler future cancels the in-flight connector call.
//!
//! Axum drops a handler's future when the client disconnects (or, here, when
//! the caller stops polling it). Nothing in this crate spawns a detached
//! task or otherwise moves connector work off that future, so dropping it
//! should stop the connector call from ever completing — not let it keep
//! running in the background. [`support::DelayedConnector`] makes that
//! observable: it flips `started` immediately and `finished` only after a
//! sleep, so a test that drops the request early and later finds `finished`
//! still false has proven cancellation, not just that it was slow to check.

mod support;

use std::sync::Arc;
use std::time::Duration;

use fabric_connector::DataConnector;
use serde_json::json;
use support::{data_sources, open_permissions, request, resolver, tenants, DelayedConnector};
use tower::ServiceExt as _;

#[tokio::test]
async fn dropping_a_request_cancels_the_in_flight_read() {
    let connector = DelayedConnector::new(Duration::from_millis(200));
    let runtime = resolver(tenants(), data_sources());
    let dispatched = Arc::clone(&connector) as Arc<dyn DataConnector>;
    let app = support::app_with_config(
        runtime,
        dispatched,
        open_permissions(),
        &fabric_data_api::DataApiConfig::default(),
    );

    let outcome = tokio::time::timeout(
        Duration::from_millis(20),
        app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"}))),
    )
    .await;

    // The timeout fired before the connector's sleep could finish, dropping
    // the handler future — and with it, the `.await` on `connector.query`.
    assert!(outcome.is_err(), "the request should not have completed in time");
    assert!(connector.started(), "the connector call must have begun");

    // Wait well past the connector's own delay. If the call were still
    // running in the background (e.g. because something had spawned it onto
    // a detached task), `finished` would flip during this sleep.
    tokio::time::sleep(Duration::from_millis(400)).await;

    assert!(
        !connector.finished(),
        "a dropped request must cancel the connector call, not let it run to completion"
    );
}

#[tokio::test]
async fn a_request_that_is_not_dropped_completes_normally() {
    // The control case: without racing a short timeout, the same connector
    // call runs to completion. This rules out `DelayedConnector` itself
    // being broken in a way that would make the cancellation test above
    // pass for the wrong reason.
    let connector = DelayedConnector::new(Duration::from_millis(20));
    let runtime = resolver(tenants(), data_sources());
    let dispatched = Arc::clone(&connector) as Arc<dyn DataConnector>;
    let app = support::app_with_config(
        runtime,
        dispatched,
        open_permissions(),
        &fabric_data_api::DataApiConfig::default(),
    );

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), http::StatusCode::OK);
    assert!(connector.finished());
}
