//! ADR 0019 §1, the half of the edge-trust contract this repository can prove.
//!
//! D01-5 accepted ADR 0019's single-runtime-route model: one gateway route per
//! runtime service, carrying one JWT policy, is the only way into
//! `/v1/data/*`, and the platform owns that edge (§G1–§G17). Nothing here
//! can prove the edge. What the runtime can prove is the other side of it:
//!
//! - it serves no path into tenant data except `/v1/data/*`, so a client
//!   instance's UI route, which the platform composes per client host, has
//!   nothing on this service to reach except the path G1 guards;
//! - every route under `/v1/data` resolves an identity before anything
//!   touches a connector, so a path that skipped the edge would still be
//!   refused here for want of a bearer;
//! - the edge's skew ceiling (§G7) is still the smallest downstream
//!   allowance.
//!
//! The requests run through the router the process serves, built by
//! `fabric_api::startup::compose` over the shipped examples.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::Router;
use examples_support::served::served_router;
use examples_support::stub_connector::StubConnector;
use examples_support::{catalog, config};
use fabric_connector::{
    ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema, DataConnector, ExecutionTarget,
    MutationOutcome, MutationSpec, QueryOutcome, QuerySpec,
};
use fabric_data_api::{required_scope, OperationKind, API_PREFIX};
use fabric_fga_auth::CLOCK_SKEW_TOLERANCE_SECONDS;
use fabric_identity::{encode_unsigned_token, LeewaySeconds};
use http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt as _;

/// ADR 0019 §G7: the edge's leeway, in seconds, is at most this.
const EDGE_LEEWAY_CEILING_SECONDS: u64 = 30;

/// The registered connector, counting every operation that reaches it.
///
/// A refusal that still reached a connector would mean a tenant's data
/// source had already been chosen, so every assertion here is paired with
/// this count.
struct CountingConnector {
    inner: StubConnector,
    operations: AtomicUsize,
}

impl CountingConnector {
    fn operations(&self) -> usize {
        self.operations.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DataConnector for CountingConnector {
    fn id(&self) -> &ConnectorId {
        self.inner.id()
    }

    fn capabilities(&self) -> &ConnectorCapabilities {
        self.inner.capabilities()
    }

    fn schema(&self) -> &ConnectorSchema {
        self.inner.schema()
    }

    async fn query(
        &self,
        target: &ExecutionTarget,
        spec: &QuerySpec,
    ) -> Result<QueryOutcome, ConnectorError> {
        self.operations.fetch_add(1, Ordering::SeqCst);
        self.inner.query(target, spec).await
    }

    async fn mutate(
        &self,
        target: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        self.operations.fetch_add(1, Ordering::SeqCst);
        self.inner.mutate(target, spec).await
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        self.inner.health().await
    }
}

fn served() -> (Router, Arc<CountingConnector>) {
    let config = config();
    let connector = Arc::new(CountingConnector {
        inner: StubConnector::new(
            config
                .connectors
                .first()
                .expect("the example configures a connector")
                .id
                .as_str(),
        ),
        operations: AtomicUsize::new(0),
    });

    let router = served_router(
        Duration::from_secs(config.request_timeout_seconds),
        Arc::clone(&connector) as Arc<dyn DataConnector>,
    );

    (router, connector)
}

/// A bearer the runtime admits for `acme`: its issuer is the one the shipped
/// registry binds to `acme`, its claim agrees, and it carries every scope the
/// example catalogue declares. Any refusal of it is about the path.
fn acme_bearer() -> String {
    let config = config();
    let issuer = config
        .identity
        .trusted_issuers
        .iter()
        .find(|registration| registration.tenant().as_str() == "acme")
        .expect("the example registers an issuer for acme")
        .issuer()
        .to_owned();

    let scopes = catalog()
        .names()
        .flat_map(|name| {
            [OperationKind::Read, OperationKind::Create]
                .map(|operation| required_scope(operation, name.as_str()))
        })
        .collect::<Vec<_>>()
        .join(" ");

    let Value::Object(claims) = json!({"iss": issuer, "tenant_id": "acme", "scope": scopes}) else {
        panic!("claims must be an object");
    };

    format!("Bearer {}", encode_unsigned_token(&claims))
}

fn request(method: &str, path: &str, bearer: Option<&str>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");

    let builder = match bearer {
        Some(bearer) => builder.header("authorization", bearer),
        None => builder,
    };

    let body = if matches!(method, "POST" | "PATCH") {
        Body::from(json!({"name": "Ada"}).to_string())
    } else {
        Body::empty()
    };

    builder.body(body).unwrap()
}

#[tokio::test]
async fn an_admitted_bearer_on_the_data_api_reaches_the_connector() {
    // The control for the test below: the same bearer, on the one path into
    // tenant data, is resolved and reaches the connector. Without it, a 404
    // below could be a bearer this router refuses rather than a path it does
    // not serve.
    let (router, connector) = served();

    let response = router
        .oneshot(request(
            "GET",
            &format!("{API_PREFIX}/customers"),
            Some(&acme_bearer()),
        ))
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(connector.operations(), 1);
}

#[tokio::test]
async fn the_runtime_serves_no_path_into_tenant_data_but_the_data_api() {
    // D01-5: a client instance's UI route adds no alternate path to
    // `/v1/data`. The runtime's share of that is to serve nothing a client
    // host could route to instead: no shell, no module, no identity or
    // session route, no tenant-prefixed or re-versioned copy of the Data API.
    // An admitted bearer goes with each request, so each 404 is the path.
    // Serving any of these is a change to ADR 0019 §1 and G1, not a route
    // to add beside them.
    let (router, connector) = served();
    let bearer = acme_bearer();

    let paths = [
        "/",
        "/index.html",
        "/shell/config.yaml",
        "/mf-manifest.json",
        "/api/user/current",
        "/api/operator",
        "/api/session",
        "/v1/identity",
        "/v1/check",
        "/acme/v1/data/customers",
        "/tenants/acme/v1/data/customers",
        "/api/v1/data/customers",
        "/v2/data/customers",
        "/data/customers",
        "/V1/data/customers",
        "/v1/Data/customers",
        "/v1/data",
        "/v1/data/",
        "/v1/datacustomers",
    ];

    for path in paths {
        for method in ["GET", "POST"] {
            let status = router
                .clone()
                .oneshot(request(method, path, Some(&bearer)))
                .await
                .unwrap()
                .status();

            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "{method} {path} must not be served"
            );
        }
    }

    assert_eq!(
        connector.operations(),
        0,
        "no path outside /v1/data may reach a connector"
    );
}

#[tokio::test]
async fn every_data_api_route_refuses_a_request_that_carries_no_bearer() {
    // The route the edge guards is also guarded here. Every method on every
    // path shape under `/v1/data` resolves an identity first, so a request
    // that reached the runtime without passing the gateway still gets 401
    // and touches nothing. A resource the catalogue does not hold is refused
    // the same way: which resources exist is not answered to a caller with
    // no identity.
    let (router, connector) = served();

    let shapes = [
        ("GET", "/customers"),
        ("POST", "/customers"),
        ("GET", "/customers/1"),
        ("PATCH", "/customers/1"),
        ("DELETE", "/customers/1"),
        ("GET", "/no-such-resource"),
        ("POST", "/no-such-resource"),
        ("GET", "/no-such-resource/1"),
        ("PATCH", "/no-such-resource/1"),
        ("DELETE", "/no-such-resource/1"),
    ];

    for (method, path) in shapes {
        let path = format!("{API_PREFIX}{path}");
        let status = router
            .clone()
            .oneshot(request(method, &path, None))
            .await
            .unwrap()
            .status();

        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} must need a bearer"
        );
    }

    assert_eq!(
        connector.operations(),
        0,
        "no unauthenticated request may reach a connector"
    );
}

#[test]
fn the_edge_leeway_ceiling_is_the_smallest_downstream_allowance() {
    // ADR 0019 §G7: the edge may tolerate at most the smallest leeway of any
    // hop behind it, or it admits tokens a downstream hop then refuses, and
    // the symptom is intermittent 401s that look like an outage. The edge's
    // own value is the platform's; the two downstream values are here. If
    // either moves, this fails and §G7's ceiling is re-derived, rather than
    // the platform learning it from production.
    let runtime_default = LeewaySeconds::DEFAULT.seconds();
    let runtime_shipped = config().leeway.seconds();
    let smallest = CLOCK_SKEW_TOLERANCE_SECONDS
        .min(runtime_default)
        .min(runtime_shipped);

    assert_eq!(
        smallest, EDGE_LEEWAY_CEILING_SECONDS,
        "the smallest downstream leeway moved; amend ADR 0019 §G7 and the platform's edge"
    );
    assert!(
        CLOCK_SKEW_TOLERANCE_SECONDS <= runtime_default && CLOCK_SKEW_TOLERANCE_SECONDS <= runtime_shipped,
        "the front door is no longer the binding constraint; ADR 0019's skew table says it is"
    );
}
