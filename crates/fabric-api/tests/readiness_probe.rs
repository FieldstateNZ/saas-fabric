//! What `/ready` tells an orchestrator, how long it takes to say it, and who
//! is allowed to hear the detail.
//!
//! Three properties, each of which the probe got wrong independently:
//!
//! 1. **The verdict must reflect what the registries hold**, not merely that
//!    they loaded. A replica holding tenants and no DataSources answers 500
//!    to every request it is sent.
//! 2. **The verdict must arrive quickly.** A kubelet `readinessProbe`
//!    defaults to `timeoutSeconds: 1`; a probe that waits on a blackholed
//!    connector is recorded as failed and the replica is pulled, which is the
//!    exact opposite of the partial-failure policy.
//! 3. **The detail must not be readable by anyone who can reach the port.**
//!    The Data API port is by design reachable by applications, so network
//!    policy draws no line here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::Router;
use examples_support::probe_connector::{Health, ProbeConnector};
use examples_support::{data_sources, tenants};
use fabric_api::health::{health_routes, HealthState};
use fabric_connector::ConnectorRegistry;
use fabric_core::SystemClock;
use fabric_core::TenantId;
use fabric_identity::{
    build_identity, encode_unsigned_token, IdentityConfig, TrustedIngressReader, TrustedIssuer,
};
use fabric_tenant_runtime::{
    DataSource, DataSourceRegistry, RuntimeResolver, TenantRegistry, TenantRuntimeBinding,
};
use http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

/// The role the example configuration grants estate-wide authority to.
const ADMINISTRATOR_ROLE: &str = "platform-admin";

/// The issuer registered to `acme`, which the probe's tokens claim.
const ACME_ISSUER: &str = "https://identity.test.invalid/realms/acme";

/// Builds the probe router over registries primed with exactly these sets.
///
/// Both registries are primed even when the set is empty: an empty published
/// set is a legitimate first load — a deployment that has onboarded nothing
/// yet — so this is reachable state, not a contrived one.
fn probe_router(
    tenants: Vec<TenantRuntimeBinding>,
    data_sources: Vec<DataSource>,
    connectors: ConnectorRegistry,
) -> Router {
    let tenant_registry = Arc::new(TenantRegistry::new());
    assert!(tenant_registry.apply_all(tenants).is_ok());

    let data_source_registry = Arc::new(DataSourceRegistry::new());
    assert!(data_source_registry.apply_all(data_sources).is_ok());

    let identity = build_identity(
        IdentityConfig {
            trusted_issuers: vec![TrustedIssuer::new(
                ACME_ISSUER,
                TenantId::try_new("acme").unwrap(),
            )],
            ..IdentityConfig::default()
        },
        Arc::new(TrustedIngressReader::new(SystemClock::shared())),
    )
    .expect("the identity configuration must build");

    health_routes(HealthState {
        runtime: Arc::new(RuntimeResolver::new(tenant_registry, data_source_registry)),
        connectors,
        identity,
        administrator_role: ADMINISTRATOR_ROLE.to_owned(),
    })
}

/// One healthy connector, which is the uninteresting case for the registry
/// tests below.
fn one_healthy_connector() -> ConnectorRegistry {
    ConnectorRegistry::new().with(Arc::new(ProbeConnector::healthy("postgres-au-east")))
}

/// A token carrying the given roles, in the trusted-ingress posture.
fn token_with_roles(roles: &[&str]) -> String {
    let claims = serde_json::json!({
        "iss": ACME_ISSUER,
        "tenant_id": "acme",
        "sub": "operator@example.com",
        "roles": roles,
        "exp": 4_102_444_800_u64,
    });

    let Value::Object(object) = claims else {
        panic!("the claims must be an object");
    };

    encode_unsigned_token(&object)
}

/// Calls `/ready`, optionally as a caller holding these roles.
async fn ready(router: Router, roles: Option<&[&str]>) -> (StatusCode, Value) {
    let mut request = Request::builder().uri("/ready");

    if let Some(roles) = roles {
        request = request.header("authorization", format!("Bearer {}", token_with_roles(roles)));
    }

    let response = router
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();

    (status, serde_json::from_slice(&bytes).unwrap())
}

/// The keys of a JSON object body, sorted.
fn keys(body: &Value) -> Vec<String> {
    let mut keys: Vec<String> = body
        .as_object()
        .expect("the probe body must be an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

// ---------------------------------------------------------------------------
// B1 — the verdict must read the registries' contents, not just their priming.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tenants_with_no_data_sources_is_not_ready() {
    // Every `resolve_data_source` for these tenants returns `MissingDataSource`
    // — a 500 `internal`, non-retryable. Answering 200 here puts a replica
    // that fails every request into rotation.
    let (status, body) = ready(
        probe_router(tenants(), Vec::new(), one_healthy_connector()),
        Some(&[ADMINISTRATOR_ROLE]),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["ready"], Value::Bool(false));
    assert_eq!(body["data_sources"], Value::from(0));
    assert!(body["data_sources_primed"].as_bool().unwrap());
}

#[tokio::test]
async fn a_deployment_with_nothing_onboarded_is_ready() {
    // The honest empty case: no tenants and no DataSources. Every request is a
    // 404 `UnknownTenant`, which is the truthful answer, and a brand-new
    // deployment must be able to start.
    let (status, _) = ready(
        probe_router(Vec::new(), Vec::new(), one_healthy_connector()),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn data_sources_with_no_tenants_is_ready() {
    // Infrastructure reconciled ahead of the first tenant. Nothing is broken:
    // there is simply nobody to serve yet.
    let (status, _) = ready(
        probe_router(Vec::new(), data_sources(), one_healthy_connector()),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn both_registries_populated_is_ready() {
    let (status, _) = ready(
        probe_router(tenants(), data_sources(), one_healthy_connector()),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// B2 — the sweep must be concurrent and bounded.
// ---------------------------------------------------------------------------

/// Comfortably longer than three concurrent 150ms checks, comfortably shorter
/// than three serial ones.
const CONCURRENCY_CEILING: Duration = Duration::from_millis(350);

#[tokio::test]
async fn the_connector_sweep_is_concurrent() {
    let connectors = ConnectorRegistry::new()
        .with(Arc::new(ProbeConnector::new(
            "postgres-au-east",
            Health::Slow(Duration::from_millis(150)),
        )))
        .with(Arc::new(ProbeConnector::new(
            "postgres-eu-west",
            Health::Slow(Duration::from_millis(150)),
        )))
        .with(Arc::new(ProbeConnector::new(
            "sqlserver-primary",
            Health::Slow(Duration::from_millis(150)),
        )));

    let started = Instant::now();
    let (status, _) = ready(probe_router(tenants(), data_sources(), connectors), None).await;
    let elapsed = started.elapsed();

    assert_eq!(status, StatusCode::OK);
    assert!(
        elapsed < CONCURRENCY_CEILING,
        "three 150ms checks took {elapsed:?}; a serial sweep is what that measures"
    );
}

#[tokio::test]
async fn a_blackholed_connector_cannot_hold_the_probe_open() {
    // At the connector's own 10s default request timeout this probe would take
    // ten seconds, and a kubelet's one-second budget would have recorded a
    // failure nine seconds earlier.
    let connectors = ConnectorRegistry::new()
        .with(Arc::new(ProbeConnector::healthy("postgres-au-east")))
        .with(Arc::new(ProbeConnector::new(
            "postgres-eu-west",
            Health::Slow(Duration::from_secs(10)),
        )));

    let started = Instant::now();
    let (status, body) = ready(
        probe_router(tenants(), data_sources(), connectors),
        Some(&[ADMINISTRATOR_ROLE]),
    )
    .await;
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(1),
        "the probe took {elapsed:?}, past a kubelet's default one-second budget"
    );

    // Unfinished is not failed. Counting it as a failure would let one slow
    // backend pull a replica that is serving every other connector — the
    // outcome the partial-failure policy exists to prevent.
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["degraded"], Value::Bool(true));

    let statuses: Vec<&str> = body["connectors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|connector| connector["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, vec!["healthy", "unknown"]);
}

#[tokio::test]
async fn every_connector_definitively_unhealthy_is_still_not_ready() {
    let connectors = ConnectorRegistry::new().with(Arc::new(ProbeConnector::new(
        "postgres-au-east",
        Health::Failing("relation tenant_ledger does not exist".to_owned()),
    )));

    let (status, _) = ready(probe_router(tenants(), data_sources(), connectors), None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

// ---------------------------------------------------------------------------
// B3 — the detail is for an authorised caller only.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_unauthenticated_caller_learns_only_whether_the_replica_is_ready() {
    let connectors = ConnectorRegistry::new().with(Arc::new(ProbeConnector::new(
        "postgres-au-east",
        Health::Failing("relation tenant_ledger does not exist on shard 3".to_owned()),
    )));

    let (status, body) = ready(probe_router(tenants(), data_sources(), connectors), None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(keys(&body), vec!["ready".to_owned()]);

    let rendered = body.to_string();
    assert!(
        !rendered.contains("postgres-au-east"),
        "connector identity is physical infrastructure: {rendered}"
    );
    assert!(
        !rendered.contains("tenant_ledger"),
        "a backend's own message must never be relayed verbatim: {rendered}"
    );
}

#[tokio::test]
async fn a_caller_without_the_administrator_role_learns_no_more() {
    let (_, body) = ready(
        probe_router(tenants(), data_sources(), one_healthy_connector()),
        Some(&["data-reader"]),
    )
    .await;

    assert_eq!(keys(&body), vec!["ready".to_owned()]);
}

#[tokio::test]
async fn an_administrator_sees_what_diagnosis_needs() {
    let connectors = ConnectorRegistry::new().with(Arc::new(ProbeConnector::new(
        "postgres-au-east",
        Health::Failing("relation tenant_ledger does not exist".to_owned()),
    )));

    let (status, body) = ready(
        probe_router(tenants(), data_sources(), connectors),
        Some(&[ADMINISTRATOR_ROLE]),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        keys(&body),
        vec![
            "connectors".to_owned(),
            "data_sources".to_owned(),
            "data_sources_primed".to_owned(),
            "degraded".to_owned(),
            "ready".to_owned(),
            "tenants".to_owned(),
            "tenants_primed".to_owned(),
        ]
    );

    let connector = &body["connectors"][0];
    assert_eq!(connector["id"], Value::from("postgres-au-east"));
    assert_eq!(connector["status"], Value::from("unhealthy"));
    assert!(connector["reason"].as_str().unwrap().contains("tenant_ledger"));
}

#[tokio::test]
async fn the_status_code_is_identical_whether_or_not_the_caller_is_authorised() {
    // A kubelet cannot present credentials and reads only the status code.
    // The two bodies differ; the verdict must not.
    let unauthenticated = ready(probe_router(tenants(), Vec::new(), one_healthy_connector()), None).await;
    let administrator = ready(
        probe_router(tenants(), Vec::new(), one_healthy_connector()),
        Some(&[ADMINISTRATOR_ROLE]),
    )
    .await;

    assert_eq!(unauthenticated.0, administrator.0);
    assert_eq!(unauthenticated.0, StatusCode::SERVICE_UNAVAILABLE);
}
